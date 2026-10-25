# Reading and writing the diagram format in PathwayDesigner — design

**Status: design for agreement, 2026-10-24. Nothing implemented.** The format is
`docs/diagram-format.md` version 0.8. Version 0.7 is published as a technical
report (Zenodo, DOI 10.5281/zenodo.23608982); 0.8 puts the diagram in a comment.
This document says how PathwayDesigner reads and writes it, what maps onto what, and the decisions needed before coding.

## 1. Scope

- **Read** a file holding an Antimony model and its `diagram` section into the
  app's model, drawn as the diagram says, with automatic layout for what it leaves
  out.
- **Write** the current diagram in the canonical form (format §12), so that reading
  it back gives the same drawing.
- **In the app:** open such files, write them, and drop them on the web page.
- **Not in the first version:** the foreseen extensions (format §15 item 9), which
  PathwayDesigner can't draw yet; several diagrams per model; modules.

## 2. Where the code goes

- `crates/pd-core/src/diagram_format/` — a new module:
  - `lex.rs`, `parse.rs`: the diagram section to a list of statements, each with
    its line and column (format §4, §16). Hand-written, like the Antimony crate's
    reader, so errors point at the place.
  - `read.rs`: statements applied to a `Model`.
  - `write.rs`: a `Model` to the canonical text.
  - `colors.rs`: the 17 basic names (format §5.1) and color constants.
- The model section is read by the existing Antimony import (`antimony::import`),
  and written by the existing `antimony::export`, so the model part is exactly what
  PathwayDesigner already reads and writes as Antimony, reserved ids included
  (the diagram is then written from the same renamed copy, so its ids agree).
- `view/interchange.rs` gains `import_diagram` and `export_diagram`, one undo step
  each, like the other formats.

## 3. Reading

1. Find the block comment whose first word is `diagram` (format §3): the diagram
   section is its contents. The whole text, comment and all, is Antimony, read as
   now. No such comment: the file is read as Antimony alone, as today. A file
   with both Antimony's layout statements and a diagram comment is drawn from the
   comment, which says more; the statements are then ignored, with one note.
2. Import the model with `antimony::import` (species are first placed at random,
   as now).
3. Read the diagram section. Problems are warnings with line and column; the
   model always loads (format §13).
4. Apply it in the canonical order, so that styles and constants exist before the
   items that use them: theme, background, color constants, styles,
   compartments, texts, species and their drawings, reactions and legs, modifier
   lines.
5. Species the diagram doesn't place are laid out automatically (D5).

**What maps onto what:**

| Format | Model |
|---|---|
| `Glc at (x, y)` | `SpeciesNode.center` |
| `size w x h` / `size d` | `width`, `height` (a diameter gives both) |
| `ATP#2` | an alias: `Model::add_alias`, its own id, `alias_of = ATP` |
| `locked` | `locked` |
| `shape`, `borderStyle` | `NodeShape`, `BorderDash` (by their `key()`s) |
| `fill`, `border`, `idColor`, `idSize`, `idBold`, `idItalic` | the item's `VisualStyle` (`has_custom_style`) |
| species `id` side / `offset` / `outside` | `label_offset`: a side computed from the id's box and the 8 gap (as `draw::label_size` and `LABEL_GAP`); `outside` by `draw::clear_label_offset` |
| `compartment C at … size …`, `hidden` | `Compartment.frame` (from the top-left corner), `hidden` |
| `text "…" at …` | a `TextItem`, id `T<n>` given in reading order |
| reaction `at` | `junction` |
| `straight` / `curved` / `linear`, `smooth` | `mode`, `junction_smooth` |
| `stem a b [free]` | `stem`, `stem_straight = !free` |
| `uses ATP#2` | the participant's `species` is the alias's id |
| `source p` / `sink p` | the null node's `center` |
| reaction `id shown` / side / `offset` | `label_offset` (`draw::clear_reaction_label_offset` for `shown`) |
| `leg … curve h1 h2` | `ctrl1`, `ctrl2` in that order (format §9 = SPECIFICATION §5.6.1), `ctrl_pts_set` |
| `leg … via` | the participant's corner points |
| `line M to R via … kind k { … }` | the modifier participant's `points`, `kind`, `look` |
| `style name kind { … }` | a `SavedStyle` (D6) |
| `theme`, `background` | `Model.theme`, `Model.background` |

**Styles.** A format style gives only some properties; a saved style in the app
is a whole look. Reading resolves each style to a whole look: its properties over
the theme's look for that kind, else the defaults. An item given one style and no
settings of its own gets that look and follows the style (`style_name`), as Apply
Style does. An item given several styles, or settings of its own, gets the
combined look and doesn't follow a style, since the app keeps an item linked only
while it looks exactly like its style (D6). A `modifier` style has no saved-style
counterpart in the app: its look is copied onto each modifier line that names it.

## 4. Writing

The canonical form of format §12, from the model:

- **Styles:** the saved styles some item follows, each written with the properties
  that differ from the defaults (or the theme's). Items following a style write
  `style <name>` and nothing else of their look.
- **Items not following a style** write the properties that differ from the
  defaults (or the theme's) in braces.
- **Positions and sizes** in the shortest form that reads back the same; sizes
  only where they differ from the item's style or the default.
- **Ids outside a node** by side when exactly there, else by offset.
- **Colors** as a basic name when one matches exactly, else hex. The file's own
  color constants and comments are not kept by the app, so they aren't written
  back (format §12 allows this; D7).
- **Aliases** numbered `#2`, `#3`, … in the order of `aliases_of`.
- **Legs:** only for participants with hand-set handles (`ctrl_pts_set`) or corner
  points.
- **Modifier lines:** each written with its kind (unless the model's interactions
  state it), its corner points and its look where it isn't the default.

## 5. Gaps found between the format and the app

1. **Hiding all modifier lines.** The app can hide every modifier line in a
   diagram (`Model.hide_modifier_lines`, saved with the diagram); the format had
   no statement for it. Added (D3): `modifierLines hidden`, a diagram-level
   statement like `background`, mapping to `Model.hide_modifier_lines`.
2. **Corner points on reactant and product lines.** The format's `leg … via`
   covers them; to check in coding that the app's field for them is the
   participant's `points` (the field's comment still says modifiers only).

Everything else the app draws has a place in the format.

## 6. In the app

- **Opening:** a `.ant` file is opened as Antimony, as now; the Antimony import
  then looks for a diagram comment and, if there is one, draws as it says. No new
  file kind is needed, and dropping a file on the web page works the same way.
- **Writing:** see D4.
- The document takes the file's name, as other imports do.

## 7. Testing

- **Unit tests** for each statement and each row of §3, and for each warning of
  format §13; deliberate breakages as usual.
- **Round trip:** every Gallery model written in the format and read back gives the
  same drawing, field by field; added to `layout_report` as a mode beside its SBML
  and Antimony modes. Expected losses: color constants and comments (none in the
  Gallery), and what D2 leaves out.
- **The report's examples:** the six examples of the technical report read without
  warnings, and draw as described; screenshots in the browser.

## 8. Steps

1. Lexer, parser and colors, with tests.
2. Reading: species, compartments, texts, reactions, legs, modifier lines, styles,
   theme, background; automatic layout of what isn't placed.
3. Writing in the canonical form; the round-trip mode of `layout_report`.
4. The app: open, write, drop; README, SPECIFICATION (a new §11.x), SPEC-Rust.
5. (If D2 says so) unknown properties and parts kept and written back.

**As built.**

- *Step 1:* `diagram_format/{mod,lex,parse,ast,colors}.rs`. The parser takes any
  clause on any line and leaves to the reader which make sense where; unknown
  parts are kept as written (`ClauseKind::Unknown`). A test reads every example
  in `docs/diagram-format.md`, which must give no warning.
- *Step 2:* `diagram_format/read.rs`, called from `antimony::import`: a diagram
  comment is used when there is one, and Antimony's layout statements are then
  ignored with a note. `Imported.had_layout` is true and `unplaced` counts the
  species drawings the diagram didn't place (left at random for now: the
  automatic layout of D5 comes with the app, step 4). New in the drawing code:
  `draw::label_offset_on_side` and `reaction_label_offset_on_side`, the sides of
  `id above` and the others, from the same helper the app's own placement uses.
  Found on the way: a second drawing doesn't follow its species' style, since
  its border is dashed by default and so it doesn't look exactly like the style
  (the app's own rule); the report's redox example puts NAD's id where its routed
  line arrives (fix in the report's next version: `NAD … { id above }`).
- *Step 3:* `diagram_format/write.rs` and `antimony::export_with_diagram`: the
  model as Antimony (the reserved-id-safe copy, so both parts use the same ids)
  and its diagram after it. The writer compares each item with a *reference*: the
  same diagram with every look, shape, handle and name as the import leaves them
  and the theme applied, which is where a reader stands before an item's own
  settings; it writes what differs. `layout_report --diagram` round-trips the
  Gallery: every model's drawing comes back exactly (what changes is the model
  part's known Antimony losses, E5). On the way: the reader applies the theme
  after placing everything, since Escher's curves and shown names depend on
  positions (it had been applied to the import's random ones); D6 revised (an
  item follows its style while it looks like it); texts in drawing order; and
  three additions to the format, 0.9, settled with the author: `off` after a
  flag (`idBold off`, `smooth off`), quoted style names (`style "Escher
  metabolite"`, so a diagram's own copy of a theme style, an Escher map's say,
  can be written and replaces the theme's), and `emptySetSize`.
- *Step 4:* the app. File ▸ Export Antimony writes the model and its diagram
  comment, and the notes on what it can't keep (a text's line breaks and double
  quotes); File ▸ Export Antimony with Layout Statements (first a checkbox in
  the File menu, replaced by a menu item at the author's request) adds Antimony's
  layout statements for tellurium and
  SBMLNetwork (`antimony::export_with_diagram_and_layout`; libAntimony 3.2.1 reads
  both forms). The panel's *Diagram* (first *With layout*) writes the diagram
  comment, and *Layout statements* beside it, asked for by the author, adds the
  statements. A text
  with both is drawn from the diagram, quietly (the format says so, §3). D5:
  species a diagram leaves out are laid out as it is read, inside
  `read::apply`, between placing and the theme: the placed ones are locked for the
  run and unlocked after, given junctions are put back, and the legs' handles,
  applied later, survive the layout's reset of them. Nothing is left for the app
  to place, so it offers no Auto-layout for such a file. Like the import's random
  placement, the layout uses the import's seed, which the app draws anew each
  time.
  Numbers are written to three decimal places (author's choice, 2026-10-24), so
  a position from automatic layout reads `81.031`, not `81.03144262477514`;
  `layout_report --diagram` compares to a thousandth, and the Gallery comes back
  within it. The one trace of the rounding: a stem whose ends were exactly in
  line (the Escher map's FRUpts2) is a hair out of line after it, so the JSON no
  longer records it as straight; drawn, the two are the same.

## 9. Decisions

- **D1. File extension.** *Decided (2026-10-24):* `.ant`, the diagram inside an
  Antimony block comment (`/* diagram … end */`), so every Antimony tool reads the
  file; format version 0.8.
*Decided by the author, 2026-10-24:* D1 `.ant` with the diagram in a comment;
D2 (a); D3 yes; D4 (a); D5 (a); D6 and D7 as written.

- **D2. Unknown properties and parts** (format §13). (a) In the first version,
  skipped with a warning and not kept; keeping and writing back later (step 5),
  which needs a place in the model and in the JSON file. (b) Keep them from the
  start.
- **D3. Hiding all modifier lines.** Add `modifierLines hidden` to the format (and
  to the report's next version). *Done in the format, 0.8 (§6.4).*
- **D4. Writing.** File ▸ Export Antimony already writes a `.ant` file, with the
  drawing as libAntimony's layout statements. With the diagram comment there are
  two ways of writing the drawing into the same kind of file. (a) Export Antimony
  writes the diagram comment instead of the layout statements, with a choice
  (a checkbox in the dialog) to write the layout statements too, for tools that
  draw from them (tellurium, SBMLNetwork). (b) Export Antimony writes both, always:
  libAntimony draws from the statements, PathwayDesigner reads the comment.
  (c) Keep Export Antimony as it is and add a separate Export ▸ Antimony with
  Diagram. In each case Save stays JSON, as now.
- **D5. Species left unplaced.** (a) Laid out automatically on opening, with the
  placed ones held where the file put them. (b) Placed at random with the
  Auto-layout offer, as for Antimony text now.
- **D6. Styles on reading.** As §3: an item with one style and no settings of its
  own follows it; otherwise it gets the combined look and follows none.
  *Revised in step 3 (2026-10-24):* an item given one style follows it whenever
  it looks exactly like it, settings of its own or not, which is the app's own
  rule for links. A writer has to give a size or an id's place beside the
  style (neither is part of a style's look while the id is inside), and under
  the first rule such an item would have lost its link on every round trip.
- **D7. Color constants and comments** are not kept by the app, so writing back
  gives the values and drops the comments.
