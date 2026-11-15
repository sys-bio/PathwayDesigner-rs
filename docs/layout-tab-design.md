# A Layout tab, with spacing — design

2026-11-15. Design; the tab itself settled with the author (rather than
Tighten/Loosen commands or a dialog per command), and T2–T5 as proposed;
T1 and T6 proposed. Step 1 built (2026-11-15): `LayoutParams::spacing`
(kept within 50%–200%) scales every method and the room beside a reaction,
and `DiagramView::layout_spacing` Straighten Chain's fixed step. On the
*E. coli* core model with cofactors separated, Stress's lines through
species fall from 185 to 117 to 64 at 75%, 100% and 150%, its crossings
staying at about 137; at 75%, Organic and Layered overlap more.

## Now

The layout controls sit in the toolbar: Auto-layout, the Method list
(docs/layout-algorithms-design.md), Deckard, Randomize and Separate
cofactors. Straighten Chain (SPECIFICATION §7.5.2) is in the Arrange menu and
keeps a chain as long as it was, which the author found too long; nothing
sets how far apart things are, for it or for the layout methods, whose
distances are fixed in the code (Stress's line 130, Layered's gaps 40 and
50, Organic's forces).

## Decisions

- **T1. The tab.** *Proposed:* a **Layout** tab in the right-hand panel,
  beside Properties, Events & Rules, Antimony, Styles and Structure, with:
  - **Auto-layout** and the **Method** list, with a line saying what the
    method does;
  - **Separate cofactors**, **Deckard**, **Randomize** (greyed as now);
  - **Spacing**, a slider (T3);
  - **Straighten Chain**: two buttons, **Vertically** and **Horizontally**,
    the same commands as in the Arrange menu, with a line saying what to
    select.
- **T2. The toolbar.** *Settled:* it keeps the **Auto-layout** button
  (used often); the Method list and the ticks move to the tab, and a small
  **Layout…** button beside Auto-layout opens it.
- **T3. What Spacing means.** *Settled:* one slider for all, as a share of each
  method's own distances, **50% to 200%**, 100% being what they are now:
  Stress's line length, Layered's gaps between neighbours and between
  layers, Organic's spring length (its repulsion scaled to match), the room
  kept for cofactors beside a reaction, and Straighten Chain's step.
- **T4. Straighten Chain's step.** *Settled:* no longer the chain's old
  length: species a fixed step apart, the species' size along the line
  plus twice 60 (so a junction sits 60 clear of each box), times Spacing.
  At 100% a vertical chain of standard species is 156 a step, about half
  what the author's trial gave.
- **T5. Live.** *Settled:* while the chain just straightened is still
  selected, moving Spacing re-spaces it at once, the same way round, and
  the whole drag is one undo step. Spacing does not re-run Auto-layout
  live (a large model takes a moment); it applies to the next run.
- **T6. Remembered.** Spacing is a preference, like the Method.

## Steps

1. Spacing in `pd-core`: `LayoutParams::spacing` used by every method and by
   Straighten Chain's fixed step (T3, T4), with tests; the report
   (`layout_quality`) at 75%, 100% and 150%.
2. The Layout tab, the toolbar (T1, T2), re-spacing live (T5), the
   preference (T6); shown in the browser.
3. Docs: help, README, SPECIFICATION §7.5.2, §10, §13.2, SPEC-Rust, release
   notes.
