//! KEGG maps (docs/kegg-import-design.md §7): every KGML file in the folder
//! `KEGG_MAPS` names is read, as one this version draws or with the reason it
//! doesn't, and a map read is written as Antimony with its diagram and as
//! SBML and read back with the same drawing. KEGG's files can't be committed
//! (design §8), so the folder is the tester's own; the test is skipped with a
//! note when it isn't set.

use std::fs;
use std::path::PathBuf;

use pd_core::antimony::{self, ImportOptions};
use pd_core::kegg;
use pd_core::model::{Model, ModifierKind};
use pd_core::sbml;

const OPTS: ImportOptions = ImportOptions {
    convert_catalytic: false,
    seed: 1,
};

fn maps() -> Option<Vec<PathBuf>> {
    let Some(dir) = std::env::var_os("KEGG_MAPS") else {
        eprintln!("note: KEGG_MAPS is not set; the KEGG map tests were skipped");
        return None;
    };
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "xml" || x == "kgml"))
        .collect();
    files.sort();
    Some(files)
}

/// A drawing as the diagram format names it: a species' id, or `ATP#2` for
/// its second drawing, whatever the alias's own id.
fn drawing_name(m: &Model, id: &str) -> String {
    let Some(s) = m.find_species(id) else {
        return id.to_owned();
    };
    match &s.alias_of {
        None => s.id.clone(),
        Some(primary) => {
            let n = m
                .aliases_of(primary)
                .position(|a| a.id == s.id)
                .unwrap_or(0);
            format!("{primary}#{}", n + 2)
        }
    }
}

/// A place, to a thousandth of a unit.
type At = (i64, i64);

/// Each drawing's name and centre, and each reaction's id, junction and
/// drawings.
type Drawing = (Vec<(String, At)>, Vec<(String, At, Vec<String>)>);

fn drawing(m: &Model) -> Drawing {
    let round =
        |p: pd_core::geometry::Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    let mut species: Vec<_> = m
        .species()
        .iter()
        .map(|s| (drawing_name(m, &s.id), round(s.center)))
        .collect();
    species.sort();
    let mut reactions: Vec<_> = m
        .reactions()
        .iter()
        .map(|r| {
            let mut ids: Vec<String> = r
                .reactants
                .iter()
                .chain(&r.products)
                .map(|p| drawing_name(m, &p.species))
                .collect();
            ids.sort();
            (r.id.clone(), round(r.junction), ids)
        })
        .collect();
    reactions.sort();
    (species, reactions)
}

fn catalysts(m: &Model) -> usize {
    m.reactions()
        .iter()
        .flat_map(|r| &r.modifiers)
        .filter(|p| p.kind == ModifierKind::Catalyst)
        .count()
}

#[test]
fn kegg_maps_are_read_and_round_trip() {
    let Some(files) = maps() else {
        return;
    };
    let mut read = 0;
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy();
        let text = fs::read_to_string(f).unwrap();
        assert!(kegg::looks_like_kgml(&text), "{name} isn't recognised");
        let i = match kegg::import(&text) {
            Ok(i) => i,
            Err(e) => {
                assert!(
                    e.contains("aren't supported yet"),
                    "{name}: an unexpected error: {e}"
                );
                continue;
            }
        };
        read += 1;
        let m = &i.model;
        assert!(i.map.warnings.is_empty(), "{name}: {:?}", i.map.warnings);
        assert!(!m.reactions().is_empty(), "{name}");
        assert_eq!(
            catalysts(m),
            m.reactions().len(),
            "{name}: one enzyme per reaction"
        );

        // Antimony with its diagram: the same drawing back.
        let ant = antimony::export_with_diagram(m);
        let back = antimony::import(&ant, &OPTS)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"))
            .model;
        assert_eq!(drawing(&back), drawing(m), "{name} through Antimony");

        // SBML: the same drawing, and the enzymes still catalysts.
        let out = sbml::export(m);
        assert!(out.problems.is_empty(), "{name}: {:?}", out.problems);
        let back = sbml::import(&out.xml, &OPTS)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"))
            .model;
        assert_eq!(drawing(&back), drawing(m), "{name} through SBML");
        assert_eq!(catalysts(&back), catalysts(m), "{name} through SBML");
    }
    eprintln!("{read} of {} KEGG maps read and round-tripped", files.len());
}
