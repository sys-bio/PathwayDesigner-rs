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
