# SBML through PathwayDesigner and back: BioModels and the SBML Test Suite

2026-10-31. What happens to a model when PathwayDesigner reads an SBML file
and writes it back, checked on every curated BioModels model and every case
of the SBML Test Suite, by simulation.

## Why this was needed

PathwayDesigner handles SBML in two layers: the SBML library (`sbml-io`),
which reads and writes all of SBML, and the translation between it and
PathwayDesigner's own model (species, reactions, compartments, parameters,
assignment rules and the drawing). CI already checks the library against the
whole SBML Test Suite with libSBML. Nothing checked the translation on models
other tools wrote: only PathwayDesigner's own models and hand-written tests.
Every defect below was in the translation.

## How

- `cargo run -p pd-core --release --example biomodels_roundtrip -- <models> <out>`
  reads each model's SBML as *File ▸ Import SBML* does and writes it back as
  *Save to SBML* does, recording what the import said.
- `crates/pd-core/tests/scripts/biomodels_check.py` compares each pair with
  libSBML (species, reactions and stoichiometry, compartments, parameters,
  rules, annotation and SBO terms) and simulates both with libRoadRunner from
  0 to 100 with tight tolerances (relative 10⁻¹⁰, absolute 10⁻¹²), comparing
  every species' concentration at 201 times.
- `crates/pd-core/tests/scripts/testsuite_check.py` does the same for the SBML
  Test Suite 3.5.0, comparing with each case's published results by the Test
  Suite's own tolerances.

Each model runs in a process of its own with a time limit, so a crash or hang
in the simulator can't stall a run. Both are run by hand (the data sets are
large); they need python-libsbml and libRoadRunner.

## Results

### Curated BioModels (1,062 models)

| Result | Models |
|---|---|
| Same behaviour after the round trip | **883** (747 bit for bit) |
| Different, and the model uses what PathwayDesigner leaves out (events, rate or algebraic rules); the import says so | 146 |
| Different with nothing left out: numerically sensitive models (below) | 2 |
| Round trip doesn't simulate: all three have events, which are left out | 3 |
| The original doesn't simulate in libRoadRunner | 26 |
| The check couldn't finish (the original makes the solver give up) | 2 |

The largest difference among the same-behaviour models that aren't bit for
bit identical is 1.7 × 10⁻⁸, where an expression is written in another but
equal form. The two that differ, BIOMD0000000883 (3.8 × 10⁻⁴) and 616
(2.4 × 10⁻⁶), differ as much from themselves when the solver's tolerance is
changed tenfold (8.2 × 10⁻⁴ and 3.7 × 10⁻⁶): they are sensitive to round-off,
not changed by the round trip.

### SBML Test Suite (1,823 cases)

| Result | Cases |
|---|---|
| Original and round trip both match the published results | **682** |
| Original matches, round trip doesn't, with nothing left out | **0** |
| Original matches, round trip doesn't: the case uses what PathwayDesigner leaves out, and the import says so | 874 |
| The original doesn't match in libRoadRunner either (mostly algebraic rules, fast reactions and delay equations, which libRoadRunner doesn't support) | 267 |

All 1,823 cases are read and written back.

## What was found and fixed

Each fix has a regression test (`crates/pd-core/tests/sbml_import.rs`, and
unit tests beside the code), checked by breaking the fix on purpose.

| Defect | Found by | Models or cases |
|---|---|---|
| Ids matched ignoring case: a reaction `R1` beside a parameter `r1` was dropped | BioModels | 11 models |
| `piecewise` and `delay` written back as calls to undefined functions, so the expression was left out | BioModels | 101 models |
| Initial assignments to species and compartments given as expressions dropped; now worked out at the start | BioModels | about 65 models |
| Species in amounts (`hasOnlySubstanceUnits`), and initial amounts, read as concentrations; both now kept, in SBML, Antimony (`substanceOnly`) and the native files (version 20) | BioModels | 8 models |
| A rule's parameter beside a compartment of the same name in another case not declared | BioModels | 8 models |
| A local parameter named as a parameter a rule sets merged with it, not renamed | BioModels | 1 model |
| libSBML crashed reading large files whose render styles listed hundreds of glyphs; a style now names at most 100 | BioModels | 16 models |
| `rateOf` (Level 3 Version 2 only) lost; such a model is now written as Level 3 Version 2, others as Version 1 | Test Suite | 51 cases |
| An initial assignment using a value an assignment rule gives left out (rules were read after them) | Test Suite | 54 cases |
| The value an initial assignment gives a species not taken in the species' own units | Test Suite | 2 cases |
| `avogadro` replaced by the newer SI value, not libSBML's Level 3 Version 1 value; now kept as the symbol, or that value beside a parameter called `avogadro` | Test Suite | 7 cases |
| A rule with no maths took the parameter it sets with it | Test Suite | 1 case |
| Conversion factors, and stoichiometry used or set in maths by its id, dropped without a word; the import now says so | Test Suite | 50 cases |

## What PathwayDesigner still leaves out

All of these are reported when a file is opened: events, rate rules,
algebraic rules, constraints, conversion factors, stoichiometry that isn't a
fixed number (read as its starting value), and other tools' own annotations.
Function definitions are written into the expressions that use them, which
keeps the model's behaviour. None of the curated BioModels models uses
conversion factors or variable stoichiometry.
