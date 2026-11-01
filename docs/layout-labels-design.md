# Label text from SBML layouts

2026-11-01. Design; decisions settled with the author (L1–L7).

Reactome names a species in full, with its compartment (`GPI dimer [cytosol]`),
and draws its map with short labels (`GPI dimer`). The short labels are in the
SBML layout: each species glyph has a text glyph whose `text` is the label.
PathwayDesigner reads where those text glyphs are but not their words, so its
diagram can only show the id (`species_70469`) or the full name. In Reactome's
glycolysis (R-HSA-70326), all 130 species' labels are their names without the
` [compartment]`.

SBML Layout lets any tool do this: a text glyph shows either the name of the
item it belongs to (`originOfText`) or words of its own (`text`). This design
keeps those words.

## What the user sees

- **Display names on (View ▸ Display names):** a drawing with label text
  shows it in place of its display name. Reactome's map then reads as Reactome
  draws it: `GPI dimer`, `G6P`.
- **The full name is kept:** the display name stays as read, in Properties and
  in the tooltip, and is saved back to SBML unchanged.
- **Properties:** a **Label text** field under Display name, for the selected
  drawing: what its label says when display names are shown. Empty means the
  display name. Typing in it is one undo step.
- **Fit to Name** fits the words shown, so it fits the short labels.

## Decisions

- **L1. Which items:** species, reactions and compartments, all of which have
  text glyphs in SBML Layout.
- **L2. When it is shown:** in place of the display name, whenever the display
  name would be shown (View ▸ Display names, or the item's Label setting); also
  for an item with label text but no display name. No new menu choice. With
  display names off, ids are shown as now.
- **L3. Per drawing:** as SBML Layout has it, a species and each of its aliases
  has its own label text. A new alias starts with its primary's.
- **L4. Reading SBML:** a text glyph's `text` becomes the label text of the
  drawing it belongs to (its `graphicalObject`), unless it is the item's
  display name or id, which need nothing kept.
- **L5. Writing SBML:** the round trip keeps Reactome's form. A drawing with
  label text gets a text glyph holding it as `text`, belonging to the drawing's
  glyph, as Reactome writes it; others keep `originOfText`, as now. Opening the
  file again gives the same labels.
- **L6. Native files:** `"labelText"` on a species, reaction or compartment,
  left out when empty; version 21.
- **L7. The diagram format:** a new property, `labelText`, in a drawing's,
  reaction's or compartment's `{ … }`:

  ```
  species_30537 at (440, 210) style metabolite { labelText "G6P" }
  compartment_70101 at (76, 87) size 2493 x 2248 { labelText "cytosol" }
  ```

  It is a property of that drawing, not a style's (a style's would make every
  drawing say the same). The diagram format document gets it in §6.2, with
  the words: what the label shows when display names are shown.

## Steps

1. **Core.** The model field, native files (version 21), SBML import and
   export, what the label shows, the tooltip. Tests, broken on purpose, and
   the Reactome file checked.
2. **App.** The Label text field in Properties. Shown in the browser.
3. **Docs.** README, help, SPEC-Rust, the JSON version history.
