# Seeing annotations on the diagram: design

Status: decisions M1–M7 agreed with the author as proposed, 2026-10-30.
Step 1 done. Step 2 done; M6 changed while building it: the selection halo
is red, not blue, and aliases are already drawn with a purple dashed border,
so *nothing found* is blue (the Structure tab's colour-blind-safe pair,
orange and blue) rather than purple. Step 3 done: the marks of what wasn't applied are
kept for the session; a later lookup replaces only the marks of the items it
looked up; a species' mark goes when it gets a ChEBI reference, a reaction's
when it gets an EC number or a Rhea reference. Step 4 done: the
specification (§5.9 tooltip, §7.6 Annotation Marks, the Annotate tab, the
Diagram group) and help.

## The problem

After Annotate Species (or Reactions) from Names, the review lists what was
found, but the diagram doesn't show which items are still to be checked or
done by hand. In a model of 77 species the user has to go back and forth
between the list and the map. And once an item is annotated, the only way to
see its annotation is to select it and open its Annotations part.

## 1. Annotations in the hover hint

The canvas already shows a hint when the pointer rests on an item for 600 ms
(S §5.9): `G6P: 1.5` for a species, `PFK: Vmax*F6P/(Km + F6P)` for a reaction,
`cell: volume 1` for a compartment. It is one line. Proposed: further lines
for its annotation, if it has one:

```
G6P: 1.5
D-Glucose 6-phosphate                    (its display name, if any)
SBO:0000247 simple chemical
is CHEBI:4170 · kegg.compound:C00092
Notes: first line of the notes…
```

References as the Annotations part shows them (`is` and the short form),
grouped by qualifier; the notes' first line, cut at about 60 letters.

## 2. Marking what still needs checking

When a lookup's searching ends and the review shows, the diagram marks:

- **Not sure**: found, but not ticked as a clear match;
- **Nothing found**.

A species is marked by a halo like the selection's but in the mark's colour
and solid; a reaction by a ring around its junction (or the middle of its
stem) and its legs drawn over in the colour, thinly. Two colours that read in
colour-blind vision: orange for *not sure*, purple for *nothing found* (the
selection halo is blue).

In the Annotate tab, under the counts:

```
Mark on the diagram:  [x] Not sure (12)   [x] Nothing found (8)
```

ticked to begin with.

## Decisions (agreed 2026-10-30)

- **M1. What the hint shows.** Display name, SBO term, references and the
  notes' first line (proposed), or fewer.
- **M2. Compartments.** Their hint shows their annotation too (proposed:
  yes, the same lines).
- **M3. How long the marks stay.** (a) Only while the Annotate tab is open;
  (b) after Apply too, for what wasn't applied, until turned off — so the
  user can work through them by hand with Find in ChEBI or the Annotations
  part, a mark going when its item gets a reference. Proposed: (b).
- **M4. Turning them off after the tab closes**, if M3 is (b): a line in the
  Diagram group's Annotations part (shown when nothing is selected), *Marked: 12
  not sure, 8 nothing found* with the two ticks and **Clear Marks**; and the
  same ticks in View ▸ Annotation Marks. Proposed: both.
- **M5. Saved?** Proposed: no, the marks belong to the session, like the
  selection; not in the file, not in exported pictures.
- **M6. The colours.** Orange and purple (proposed), or others.
- **M7. Ticked rows.** A row ticked by the user but not applied (Cancel) is
  not marked; Cancel removes all the marks of that lookup. Proposed.

## Steps

1. The hint (M1, M2).
2. The marks: drawing them, the ticks in the tab (M6, M7).
3. The marks after Apply and their controls (M3–M5).
4. The specification and help.
