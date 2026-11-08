# All of SBML through Antimony: step 3

2026-11-07. Design for step 3 of docs/sbml-full-support-design.md; S1–S7
settled with the author as proposed (S4: the block holds only what Antimony
can't say; S6: saving as usual writes the model, the diagram and the block). Settled before it: "Load Antimony" carries over what
Antimony can't say, from the model it replaces; PathwayDesigner's Antimony
diagram format carries it too, so a saved file is complete.

## Now

Antimony reaches PathwayDesigner three ways: the Antimony tab (Get Antimony,
Load Antimony, which replaces the model and keeps the drawing), opening and
saving a `.ant` file, and the diagram format, which is Antimony with the
drawing in a comment. All three go through `pd-core/src/antimony.rs`, which
since step 1 reads and writes only what the model held before: species,
reactions, compartments, parameters, assignment rules, names and
annotations. Events, rate and algebraic rules, constraints, functions and
units are skipped with a warning on reading and not written.

## What Antimony can say, and can't

The Antimony crate reads and writes, and converts to SBML: events (trigger,
delay, priority, `persistent`, `t0`, `fromTrigger`), rate rules (`x' = …`),
algebraic rules, constraints with messages, functions, unit definitions and
`has` units, conversion factors, `const` and `var`, initial values given by a
formula, names (`is`) and annotations (`sboTerm`, `identity` and the other
qualifiers, `notes`, creators and dates) on anything with a name.

It can't say, and SBML has: a rate law's own local parameters; `fast`;
species references' ids, names and annotations; annotations on what has no
name in Antimony (rules, initial assignments, rate laws, a trigger, delay or
priority); the model's own units; a unit's scale apart from its multiplier;
values a file left unset; the model's id beside its name; metaids; other
tools' annotations, RDF the reader doesn't take apart, other packages' parts.

## Decisions

- **S1. What Antimony can say is written and read as Antimony.** Everything
  in the first list above, both ways, as libAntimony writes it. *Proposed.*
- **S2. Local parameters in the text.** A rate law's own `k` is written as a
  model parameter `J1_k` (the next free name if that is taken) and the law
  uses that name, so the text means the same in any Antimony tool.
  *Proposed.*
- **S3. Load Antimony: what the text leaves alone stays as it was.** For each
  item (species, compartment, parameter, reaction, rule, initial assignment,
  event, function, unit, constraint), if the text says of it exactly what Get
  Antimony would write for it, the item is kept as it was, with everything
  Antimony can't say: its annotations, a reaction's local parameters (`J1_k`
  back to J1's own `k`), its species references, numbers written as the file
  had them. An item the user changed is taken from the text, keeping its
  annotations and names, which still apply; a reaction whose rate law changed
  keeps those of its local parameters the new law still uses. *Proposed.*
- **S4. A second comment block, not the diagram.** The diagram format's first
  principle is that the diagram never restates the biology: remove it and
  the model is unchanged (docs/diagram-format.md §2). Local parameters and
  annotations are biology. *Proposed:* what Antimony can't say goes in a block
  of its own after the diagram, `/* sbml … end */`, written whenever the model
  has any, read with or without a diagram. Like the diagram, every Antimony
  tool skips it. The diagram format's specification gains a section for it,
  and its version goes up.
- **S5. The block's lines.** *Proposed:* words for what a modeller may read or
  write: `J1_k local J1 k` (the parameter `J1_k` is J1's own `k`), `fast J1`,
  `units substance mole, time minute, …` for the model's, `S1 unset initial`.
  For the purely technical rest, which no one writes by hand (metaids, other
  tools' XML, other packages' parts, annotations on species references, rules
  and rate laws, unit scales), one line per item, `S1 sbml { … }`, holding
  what the native file holds for it (SPEC-Rust §4, version 23).
  *Alternative:* words for everything, which makes the block much longer and
  needs a word for each of some 40 SBML details.
- **S6. Plain Antimony.** Saving a model as plain Antimony (no diagram, no
  block), or Get Antimony without the diagram, says what the text leaves out
  when the model has any of it. In the app, S3 keeps it anyway.
  *Proposed.*
- **S7. The catalytic rewrite** isn't applied when reading Antimony either
  (F6). *Proposed.*

## Done means

- The round-trip check (F14) through Antimony: each of the 1,062 curated
  BioModels and 1,823 Test Suite cases read from SBML, written as Antimony with
  the block, read back and written as SBML, compared with the original by
  `crates/pd-core/tests/scripts/sbml_compare.py`: no difference. Through plain
  Antimony, the differences are only those S6 names.
- Load Antimony after Get Antimony, with nothing edited, leaves the model
  exactly as it was (S3), checked on the same models.
- Tests for each part, each broken on purpose.

## Steps

1. The bridge: S1, S2, S7, both ways; unit tests.
2. Load Antimony keeping what the text leaves alone (S3).
3. The block (S4, S5) and S6; the round-trip check through Antimony.
4. The diagram format's specification and report.
