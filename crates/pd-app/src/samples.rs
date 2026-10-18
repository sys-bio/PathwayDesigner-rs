//! The reference models from `Models/`, built into the app so they can be
//! opened before file access arrives (R11).

/// (name, JSON text)
pub const SAMPLES: &[(&str, &str)] = &[
    ("model1", include_str!("../../../Models/model1.json")),
    ("model10", include_str!("../../../Models/model10.json")),
    ("model12", include_str!("../../../Models/model12.json")),
    (
        "model13_yejee1",
        include_str!("../../../Models/model13_yejee1.json"),
    ),
    ("model6", include_str!("../../../Models/model6.json")),
    ("model9", include_str!("../../../Models/model9.json")),
    (
        "modelModifier1",
        include_str!("../../../Models/modelModifier1.json"),
    ),
    ("modtest", include_str!("../../../Models/modtest.json")),
];
