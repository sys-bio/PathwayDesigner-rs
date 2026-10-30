# Getting started

There are two ways to make a network: write it in Antimony, or draw it.

## Write a model in Antimony

1. In the panel on the right, choose the **Antimony** tab.
2. Type a model, or paste one in. For example:

   ```
   S1 -> S2; k1*S1
   S2 -> S3; k2*S2
   k1 = 0.1; k2 = 0.2
   S1 = 10
   ```

3. Press **Load Antimony**. The species and reactions appear on the
   canvas. If the text says nothing about where they go, they are placed at
   random; press **Auto-layout** under the text to arrange them.

If a line has a mistake, it is marked in the text and listed below it. Click
the message to go to the line. The drawing is not changed until the model
loads without errors.

## Draw a model

1. Choose **Species** in the tools on the left and click on the canvas, once
   for each species.
2. Choose a reaction tool, such as **A -> B**, and click the species in turn:
   first the reactant, then the product.
3. Choose **Select** (or press Esc) to select, drag and change things. The
   **Properties** tab on the right shows what is selected, and right-clicking
   an item gives a menu of what can be done to it.

## See the drawing as Antimony

Press **Get Antimony** in the Antimony tab to see the network you have drawn
written as an Antimony model. Tick **Diagram** first to have the drawing
written too: where each item is and how it looks, in a short comment after
the model. Any Antimony tool still reads the model, and PathwayDesigner reads
the drawing back when the text is loaded again.

## Ids and display names

Every species, reaction and compartment has an **id**, the name that rate laws
use, such as `S1`. It can also have a **display name**, any words you like,
such as *glucose*. Select an item and type its display name in the
**Properties** tab. **View ▸ Display names** then labels the diagram with the
display names instead of the ids; the **Label** list in the Properties tab
changes this for the selected items only. Models read from SBML or Antimony
keep the display names they have.

## Annotations

A species, reaction or compartment can also say what it *is*: an SBO term
(247 is a simple chemical), references to database entries (ChEBI, UniProt, a
paper in PubMed) and notes. Select an item and open **Annotations** in the
**Properties** tab to see and change them; with nothing selected, the
**Diagram** group shows the model's own, and who made it. A reference can be
typed as a link or a short identifier such as `CHEBI:15422`, and clicking it
opens the entry in your browser. Annotations come and go with SBML and
Antimony files.

PathwayDesigner can also look species and reactions up for you, in BiGG,
ChEBI, PubChem and Rhea:

- **Find in ChEBI**, in a species' Annotations, searches for its name (or any
  words you type) and lists what it finds; **Add** puts one in.
- **Edit ▸ Annotate Species from Names…** (also on a species' right-click
  menu) looks up the selected species, or all of them, and shows what it found
  in an **Annotate** tab. Clear matches are ticked; check the others, since a
  short name can find the wrong thing (click a species' name to see it on the
  diagram). **Apply** adds what is ticked, in one step you can undo.
- **Edit ▸ Annotate Reactions from Names…** (also on a reaction's right-click
  menu) does the same for reactions: it finds their EC numbers and their Rhea,
  KEGG and MetaNetX entries, by the reaction's id in BiGG or by its species in
  Rhea. Compare the equation shown with your reaction before ticking it.
  Annotate the species first: Rhea is searched by the species' ChEBI entries.

While you review, and after **Apply**, the diagram marks what is left to do:
an **orange** ring for an item found but not ticked, a **blue** one for an
item where nothing was found. Work through them with **Find in ChEBI** or by
typing references; a mark goes when its species gets a ChEBI reference, or
its reaction an EC number or Rhea entry. **View ▸ Annotation Marks** turns
each kind on or off and clears them; marks aren't saved.

Rest the pointer on an item to see its annotation in the hint.

## Finding an item

The list on the left, below the tools, has every species, reaction and
compartment. Type in its **Find** box (or press Ctrl+F) to keep only those
whose id or display name holds the words; click one, or press Enter for the
first, to select it and bring it into view.

## Open an example

**File ▸ Open from Gallery…** opens example models, from small networks to
*E. coli* central metabolism. They are a quick way to see what a finished
diagram looks like.

## Save your work

- **File ▸ Save** keeps the model and its drawing in PathwayDesigner's own
  file.
- **File ▸ Export Antimony…** writes an Antimony file (`.ant`) with the
  drawing in a comment after the model.
- **File ▸ Save to SBML…** writes SBML, with the drawing in SBML's Layout and
  Render parts, for other tools such as COPASI.

Back to the [contents](contents.md).
