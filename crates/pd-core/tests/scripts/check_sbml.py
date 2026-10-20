"""Checks SBML files with python-libsbml's checkConsistency() (SPEC-Rust
§8.3): no errors allowed, warnings reported. Also checks each file has a
layout with a glyph for every species. Ends with the warnings counted by
kind. Exits with 1 on any error.

    python check_sbml.py <folder of .xml files>
"""
import collections, os, sys, libsbml

folder = sys.argv[1]
bad = 0
kinds = collections.Counter()
where = collections.defaultdict(set)
files = sorted(f for f in os.listdir(folder) if f.endswith(".xml"))
for f in files:
    doc = libsbml.readSBMLFromFile(os.path.join(folder, f))
    doc.checkConsistency()
    found = [doc.getError(i) for i in range(doc.getNumErrors())]
    errors = [e for e in found if e.getSeverity() >= libsbml.LIBSBML_SEV_ERROR]
    for e in found:
        if e.getSeverity() < libsbml.LIBSBML_SEV_ERROR:
            key = (e.getErrorId(), e.getCategoryAsString(), e.getShortMessage().strip())
            kinds[key] += 1
            where[key].add(f)
    warnings = len(found) - len(errors)
    model = doc.getModel()
    layouts = model.getPlugin("layout").getNumLayouts() if model else 0
    glyphs = model.getPlugin("layout").getLayout(0).getNumSpeciesGlyphs() if layouts else 0
    if errors or layouts != 1 or glyphs < model.getNumSpecies():
        bad += 1
        print(f"{f}: {len(errors)} errors, {layouts} layouts, {glyphs} species glyphs")
        for e in errors[:5]:
            print("   ", e.getMessage().strip()[:300])
    else:
        print(f"{f}: ok ({warnings} warnings, {glyphs} species glyphs)")
print(f"{len(files) - bad} of {len(files)} files pass")
if kinds:
    print("Warnings by kind (id, category, message: count in files):")
    for (eid, cat, msg), n in sorted(kinds.items()):
        print(f"  {eid} {cat}: {msg}: {n} in {len(where[(eid, cat, msg)])} files")
sys.exit(1 if bad or not files else 0)
