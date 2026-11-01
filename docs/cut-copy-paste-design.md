# Cut, Copy and Paste of parts of a network

2026-11-01. Design; decisions C1–C4 settled (C1 and C2 as recommended, C3 and
C4 later).

PathwayDesigner can copy and paste a *style* (Ctrl+Alt+C, Ctrl+Alt+V) but not
the things themselves. This adds Cut, Copy and Paste for the selection:
species, reactions, compartments and text, with what they need to work
(rate laws, parameters, annotations, looks).

## What the user sees

- **Edit ▸ Cut, Copy, Paste**, with Ctrl+X, Ctrl+C, Ctrl+V, and the same three
  in the canvas's right-click menu. Cut and Copy are greyed out with nothing
  selected; Paste with nothing on the clipboard PathwayDesigner can read.
- **Paste puts the copy under the mouse** if the mouse is over the canvas,
  otherwise a little below and right of where the originals were (or of the
  last paste, so pasting three times gives three copies, not one on top of
  another). The pasted items are left selected, so they can be dragged at once.
- **Cut** is Copy then Delete, one undo step. **Paste** is one undo step.
- **Between documents and windows.** The copy goes on the system clipboard, so
  it can be pasted into another PathwayDesigner window or browser tab, or into
  the same window after opening another model.

## What is copied

- **The selection, plus what a reaction can't be drawn without.** A selected
  reaction brings its reactants, products and modifiers along even if they
  weren't selected (decision C1).
- **Rate laws** of the copied reactions, and the **parameters** and
  **assignment rules** they use, directly or through other rules.
- **Species' values** (initial value, boundary, in amounts), **display names**,
  **annotations**, the names of the annotations' entries, and **looks** (style,
  shape, colours, fonts, label position). Saved styles an item links to come too.
- **Compartments** that are selected, with the species inside them that are
  selected. A copied species whose compartment isn't copied goes into the
  compartment it lands in when pasted, or the default compartment.
- **Aliases.** An alias copied with its primary stays an alias of the copy.
  An alias copied without its primary becomes an alias of the primary in the
  document it is pasted into, if that has a species of that id; otherwise the
  first such alias pasted becomes a primary.
- **Text items** that are selected.

## Ids

Ids must stay unique, and the copies are new things (decision C2):

- An id not used in the document keeps its name; this is the usual case when
  pasting into another model.
- An id already used gets the first free `_2`, `_3`, … suffix: `ATP` → `ATP_2`.
  Display names are copied unchanged, so the diagram still reads "ATP".
- Rate laws and rules are rewritten to use the new ids, with the same code
  that renaming an id uses now (`Model::rename_species` and friends).

## On the clipboard

The copy is written as a small PathwayDesigner JSON document (the native
format, the same version as a saved file) holding only the copied items, with
a marker saying it is a PathwayDesigner clipboard fragment. Reading it back
uses the native reader, so nothing new needs its own format and old clipboards
stay readable. Plain text that isn't a fragment is ignored by Paste in the
canvas (decision C3).

On the web, the browser's copy, cut and paste events carry the text (egui's
`Event::Copy`, `Event::Cut`, `Event::Paste`); the desktop app uses the system
clipboard the same way. Ctrl+C in a text field still copies the field's text,
not the diagram.

## Decisions

- **C1. A reaction without its species.** *Recommended:* bring its species along
  (Copy always gives something that works). The alternative is to paste the
  reaction joined to species of the same id already in the document, as
  CellDesigner does; that is useful for joining a pasted step onto an existing
  pathway, but confusing when the species aren't there.
- **C2. Parameters used by copied rate laws, when pasting into the same
  document.** *Recommended:* the copies get their own parameters (`k1` →
  `k1_2`), so changing one copy doesn't change the other. The alternative is to
  share a parameter whose name and value match.
- **C3. Pasting Antimony text.** Not in this design. Later, Paste could also
  read Antimony copied from a text editor and add it to the diagram.
- **C4. Edit ▸ Duplicate (Ctrl+D).** Copy and Paste in one step, leaving the
  clipboard alone. Cheap to add once Paste works; leave out unless wanted.

## Steps

1. **Core.** `pd-core`: make a fragment from the selection (with what C1 brings
   along), and paste a fragment into a model with ids made unique, laws
   rewritten, positions moved and the pasted items selected; one undo step.
   Tests for each rule above, broken on purpose.
2. **App.** Edit menu and context menu items, shortcuts, the clipboard on the
   web and desktop, the paste position. Shown in the browser.
3. **Docs.** Help window, getting-started help, README Mouse and keyboard rows,
   SPEC-Rust, release notes.
