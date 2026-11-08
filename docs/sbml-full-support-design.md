# Full SBML core: nothing in a file is lost

2026-11-06. Design. All decisions settled with the author (F8 and F9 changed
from the first proposal). Step 1 built (2026-11-06): pd-core's model holds
every part (`model_core.rs`), renames and deletes reach them (F8), and the
native file keeps them (version 23, SPEC-Rust §4). Step 2 built
(2026-11-06): SBML import and export hold everything; checked by
`crates/pd-core/tests/scripts/sbml_compare.py` (F14), all 1,062 curated
BioModels and 1,823 Test Suite cases with no difference element by element,
none with an error libSBML doesn't find in the original. Step 3 built
(2026-11-07): Antimony holds everything, with an `sbml` block for what it
can't say (docs/antimony-full-support-design.md); the same check through
Antimony, and through Get and Load Antimony, finds no difference. Step 4
built (2026-11-07): the app (docs/sbml-app-design.md). Step 5, the
documents, is next.

The rule this design serves: **a model PathwayDesigner reads from SBML and
saves, unchanged by the user, means exactly what the file meant.** Editing
changes only what the user edits. A warning on opening is not a substitute.

## Now

PathwayDesigner's model was made for drawing and structure analysis. It holds
compartments, species, reactions, parameters (an id and a value or formula)
and assignment rules. The SBML reader (`sbml-io`) reads everything, but the
import keeps only what that model holds, and the export builds a new document
from it, so the rest is gone on saving. Some losses are reported on opening;
many are silent.

Changes in meaning (simulation results differ):

1. **Events** dropped (reported).
2. **Rate rules** dropped (reported); the variable is then saved as constant
   with its starting value (silent).
3. **Algebraic rules** dropped (reported); their variables saved as constant.
4. **A rate law dropped**, leaving a reaction with none, when its maths uses
   `<infinity/>`, `<notanumber/>`, an unknown csymbol or MathML, or a function
   definition that can't be read (reported).
5. **`fast="true"`** written as `false` (silent).
6. **Species reference ids** replaced by new ones, so maths that uses or sets a
   stoichiometry by its id now names a new parameter with value 0 (reported
   only as "read with the stoichiometry it starts with"). Variable
   stoichiometry becomes a number.
7. **Parameter values INF and NaN** become 0 (a note on export only).
8. **Conversion factors** dropped (reported).
9. **Initial assignments to species and compartments** worked out once and
   saved as numbers (silent); the formula is lost, and the value is wrong
   when it uses a species given as an amount in a compartment whose size
   isn't 1.
10. **Missing values filled in** (silent): a parameter without a value gets 0,
    a compartment without a size 1, a species without an initial value 0, a
    stoichiometry 1.
11. **Non-constant parameters and compartments** (set by an event or a rate
    rule) written `constant="true"` (silent).
12. **The `time` csymbol and a parameter called `time`** become one thing;
    `true`/`false` outside a piecewise condition become 1/0 (silent).
13. **Catalytic rewrite on import** (on by default): a species on both sides
    of a reaction is taken off both and made a modifier (silent).

Information lost without changing results (silent unless noted):

- **Units**: unit definitions, the model's units, every `units` attribute,
  units on numbers.
- **Function definitions** written into the expressions that use them
  (reported); the definitions themselves are gone.
- **Local parameters** made global, renamed when the id is taken.
- **Constraints** (reported).
- **The model's id** replaced by one made from its name.
- **Names** of parameters, local parameters, species references, modifiers,
  functions, events and unit definitions.
- **Notes, annotations, SBO terms and metaids** on everything except the model,
  compartments, species, parameters and reactions: rules, initial assignments,
  events, functions, units, kinetic laws, species references, modifiers.
  Modifiers' SBO terms are rounded to a few kinds.
- **Level 2 only**: compartment `outside`, species `charge`, compartment and
  species types. Every file is saved as Level 3.
- **The reaction's `compartment`**, the default compartment's name and notes.
- **Other SBML packages** (fbc, qual, comp, …): their attributes and elements on
  every item, and the namespaces on `<sbml>`.

The full list, with the code for each, was made on 2026-11-06 from
`pd-core/src/sbml/import.rs`, `export.rs` and `math.rs`.

What already exists: `sbml-io` reads and writes all of SBML core, and keeps
what it doesn't understand (`Meta.extra`). The Antimony crate reads, writes
and converts to SBML events, rate and algebraic rules, constraints, units,
`const`/`var`, function definitions and initial assignments. Only pd-core's
model, and the import, export and Antimony bridge around it, lack them.

## What changes

PathwayDesigner's model holds all of SBML Level 3 core. Each part can be seen
in the model tree, edited, written to and read from Antimony and the native
file, and written to SBML as it was read.

- **Added to the model:** events (trigger with its initial value and
  persistence, delay, priority, use values from trigger time, assignments);
  rate rules; algebraic rules; constraints with their messages; function
  definitions; unit definitions; initial assignments to anything, kept as
  formulas; conversion factors.
- **Added to existing items:** a name and units on every item that SBML allows
  them on; `constant` on parameters, compartments and species references;
  `fast` and `compartment` on reactions; local parameters on rate laws; ids,
  names and stoichiometry rules on species references and modifiers; the model's
  own id beside its name; "not set" for values a file leaves unset.
- **Notes, annotations, SBO terms and metaids on every item**, as the five
  kinds of item have them now (references, notes, other tools' annotations).
- **Maths** keeps `inf`, `NaN`, `true`, `false`, the time symbol and a
  parameter called `time` apart, and any csymbol it doesn't know as written.
- **Other packages** kept, as other tools' annotations are now (F10).

## Decisions

- **F1. Function definitions.** *Proposed:* kept as definitions, and rate laws
  call them, as written. Structure analysis and the drawing don't depend on
  them. *Settled:* as proposed, with a preference to write functions into
  the expressions that use them instead, off by default.
- **F2. Local parameters.** *Proposed:* kept local to their reaction, shown
  with the rate law. A local parameter with a global's id stays separate, as
  SBML says. *Settled:* as proposed.
- **F3. Initial assignments.** *Proposed:* kept as formulas for species,
  compartments, parameters and species references. The value they give at the
  start is worked out and shown beside the formula, not stored in its place.
  *Settled:* as proposed.
- **F4. Units.** *Proposed:* unit definitions and every units attribute held
  and written back; shown in the property panel and chosen from the model's
  units. No unit checking or conversion in this version. *Settled:* as
  proposed.
- **F5. The SBML level written.** Files are written as Level 3 now.
  *Proposed:* keep that (Version 2 throughout, which has everything Version 1
  has), and write every value that Level 2 leaves to a default explicitly, so a
  Level 2 file keeps its meaning. Level 2-only attributes (`outside`,
  `charge`, compartment and species types) have no Level 3 form; none changes
  results, and they are reported on opening, the one exception to "nothing is
  lost". *Alternative:* write a Level 2 file back as Level 2, which keeps them
  but means writing both levels. *Settled:* always Level 3 Version 2.
  *Amended (step 2):* Version 2 has no `fast` attribute on reactions
  (libSBML refuses it), so a model with a fast reaction is written as
  Level 3 Version 1, which keeps it.
- **F6. The catalytic rewrite.** It changes the file's reactions. *Proposed:*
  never on reading SBML or Antimony; available as a command, as Place Modifiers
  is. The preference goes. *Settled:* as proposed.
- **F7. Modifiers added from the rate law.** A species the rate law uses that
  isn't a participant is made a modifier on import. That adds to the file but
  doesn't change its meaning, and SBML recommends it. *Proposed:* keep doing
  it. *Settled:* as proposed.
- **F8. Edits that break references.** An event, rule, constraint or initial
  assignment may name an item. *Proposed:* renaming an item renames it
  everywhere, as rate laws are now; deleting an item that something names
  asks first, listing the uses, and leaves those formulas naming an item that
  no longer exists, marked as errors in the tree (not deleted with it).
  *Settled:* renamed everywhere; what names a deleted item is deleted with
  it, only where keeping it would make the model invalid SBML. So: a rule, initial assignment, constraint or
  event assignment whose formula uses the item, or that sets it, is deleted;
  an event whose trigger, delay or priority uses it is deleted whole; an
  event left with no assignments stays. Rate laws are treated as now. One
  undo brings it all back, and a message says what went with the item.
  *Settled.*
- **F9. Where the new parts are shown.** *Proposed:* the model tree gets
  Events, Rules, Constraints, Functions and Units branches; each opens in the
  property panel for editing; the Antimony tab shows and edits them all. The
  diagram draws nothing new, except that a species or compartment set by a
  rule or an event is marked (a small symbol, as annotation marks are).
  *Not taken:* the author: a model tree is a computer-science device that
  may not suit biologists. *Proposed instead:* a right-panel tab, **Events &
  Rules**, listing them in words under plain headings, each as a sentence
  ("When time > 10: Glucose becomes 5"; "Volume changes at the rate
  k*Glucose"), with Functions, Units and Constraints ("Checks") below.
  Clicking a sentence edits it in place. A species' or compartment's
  properties say what else changes it ("Also changed by: the event Feed"),
  and clicking that goes to it. The Antimony tab shows them all as text;
  the diagram mark stays. *Settled:* as proposed instead.
- **F10. Other packages** (fbc, qual, comp, …). *Proposed:* their content on
  each item kept and written back, with their namespaces, as other tools'
  annotations are (docs/other-annotations-design.md): kept with the item,
  gone with it, not checked after edits. The `comp` package's submodels are
  the exception: a model built from submodels is reported on opening as not
  editable. fbc may get real support later. *Settled:* other packages kept.
- **F11. Structure analysis.** A species set by a rate or assignment rule, or
  changed by an event, isn't governed by the reactions alone. *Proposed:*
  analysed as now (rules and events don't enter the stoichiometry matrix),
  with the Structure tab saying which species are also set by rules or
  events. *Settled:* analysis unchanged, with the note.
- **F12. Simulation.** PathwayDesigner doesn't simulate; nothing changes.
- **F13. Native file and the diagram format.** *Proposed:* the native file
  (JSON) gets a new version holding all of the above; older files open as now.
  PathwayDesigner's Antimony diagram format needs no change: the new parts are
  Antimony, and the drawing comments don't touch them. *Settled:* a new
  JSON version.
- **F14. The test of "nothing is lost".** *Proposed:* a check, run on the
  1,062 curated BioModels and the 1,823 Test Suite cases, that reads each file,
  saves it and compares the two models element by element with libSBML
  (ignoring layout, render, and differences SBML defines as equal, such as
  an attribute left out and the same value written). Done means: no difference
  in any file, and every case libRoadRunner passes before the round trip
  passes after it. The same check through Antimony and through the native
  file. Added to the tests that are run before a release. *Settled:* as
  proposed.

## Steps

1. **The model and the native file.** The new parts and attributes in pd-core's
   model, the JSON version, undo, renames and deletes (F8). Unit tests.
2. **SBML.** Import and export of everything, maths included; the round-trip
   check (F14) on the Test Suite and BioModels, every difference fixed.
3. **Antimony.** The bridge reads and writes everything the model holds; the
   round-trip check through Antimony.
4. **The app.** Tree branches, property panel editing, marks on the diagram
   (F9), the Structure tab note (F11), the opening warnings that remain.
5. **Documents.** SPEC-Rust, SPECIFICATION, help, README; the paper's
   Appendix B and Limitations rewritten with the new results.

Each step is tested, including by breaking it on purpose, and shown in the
browser where it changes the app.
