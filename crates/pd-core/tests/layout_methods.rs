//! The layout methods on a real model (docs/layout-algorithms-design.md):
//! BiGG's *E. coli* core model, read as a model without a drawing is.

use pd_core::antimony::ImportOptions;
use pd_core::layout::{LayoutMethod, LayoutParams, LayoutRun, separate_cofactors};
use pd_core::layout_measures::measure;
use pd_core::model::Model;

const MAP: &str = include_str!("../../../Escher/e_coli_core.Core metabolism.json");

/// The core model without its drawing: through Antimony, species at random.
fn core() -> Model {
    let drawn = pd_core::escher::import(MAP).unwrap().model;
    let text = pd_core::antimony::export(&drawn);
    pd_core::antimony::import(&text, &ImportOptions { seed: 1 })
        .unwrap()
        .model
}

fn lay_out(mut m: Model, params: LayoutParams) -> Model {
    if params.separate_cofactors {
        separate_cofactors(&mut m);
    }
    let mut out = m.clone();
    LayoutRun::new(&m, params, 1).finish(&mut out);
    out
}

#[test]
fn stress_on_the_core_model() {
    let params = LayoutParams {
        method: LayoutMethod::Stress,
        ..LayoutParams::default()
    };
    // No box on another, with or without cofactors separated, and fewer
    // crossings with them separated.
    let plain = measure(&lay_out(core(), params));
    assert_eq!(plain.overlaps, 0, "{plain:?}");
    let separated = measure(&lay_out(
        core(),
        LayoutParams {
            separate_cofactors: true,
            ..params
        },
    ));
    assert_eq!(separated.overlaps, 0, "{separated:?}");
    assert!(
        separated.crossings * 4 < plain.crossings,
        "{} against {}",
        separated.crossings,
        plain.crossings
    );
}
