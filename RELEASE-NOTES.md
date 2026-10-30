# Release notes

What changed in each version, for the GitHub release. The packages for
Windows, macOS and Linux and the web zip are attached to each release; the web
version at https://sys-bio.github.io/PathwayDesigner-rs/ is always the latest.

## 0.1.28 (2026-10-30)

**New**

- **Annotate Species from Names.** Edit ▸ Annotate Species from Names… (also
  on a species' right-click menu) looks the selected species, or all of them,
  up in BiGG, ChEBI and PubChem, and shows what it found in an *Annotate* tab:
  the ChEBI entry for each, its full name, and BiGG's KEGG and MetaNetX links.
  Clear matches are ticked; the others are offered to check. **Apply** adds
  what is ticked, references and display names, in one step you can undo. On
  the Millard *E. coli* model it ticks 17 species, all correctly.
- **Annotate Reactions from Names.** The same for reactions (Edit menu, or a
  reaction's right-click menu): EC numbers and Rhea, KEGG and MetaNetX
  reactions, found by the reaction's id in BiGG or by its species in Rhea. The
  review shows each match's equation, to compare with the reaction.
- **Find in ChEBI**, in a species' Annotations, searches ChEBI for its name or
  any words typed; **Add** puts an entry in.
- **Marks on the diagram.** While a review is open, and after Apply, the
  diagram marks what is left to check: an orange ring for an item found but
  not ticked, a blue one for nothing found. A mark goes when its item gets a
  ChEBI reference (a species) or an EC number or Rhea entry (a reaction).
  **View ▸ Annotation Marks** turns each kind on or off and clears them.
- **Annotations in the hint.** Resting the pointer on a species, reaction or
  compartment shows its display name, SBO term, references and notes.
- **Find in the list.** The list on the left has a Find box (Ctrl+F): type to
  keep only the items whose id or display name hold the words; picking one
  selects it and brings it into view.

**Changed**

- The SBO terms offered in an item's Annotations suit the item; the model's
  row is *Framework (SBO)*, listing the modelling frameworks.
- Links to database entries open in a new browser tab, never over the app.

## 0.1.27 (2026-10-28)

**New**

- **Annotations are kept.** What a curated model says each item *is* now
  survives opening and saving: SBO terms, database references (links to
  ChEBI, UniProt, KEGG, PubMed and the like) and notes on species, reactions,
  compartments, parameters and the model, and the model's creators and dates.
  They are read from and written to SBML and Antimony, and saved in
  PathwayDesigner's files (version 18). Five curated BioModels models come
  back through PathwayDesigner with every one unchanged. Before, they were
  dropped without a word.
- **Annotations in the panels.** Each species, reaction and compartment, and
  the model (in the Diagram group, with nothing selected), has an
  *Annotations* part: the SBO term with its name and a list of common ones,
  the references as links that open in the browser, with a way to add one
  (a link, or an identifier such as `CHEBI:15422`) and remove one, and the
  notes as text. The Diagram group also shows who made the model and when.
- **KEGG maps are annotated.** Compounds refer to their KEGG entries and, once
  named, their PubChem compounds; enzymes to their KEGG genes; reactions to
  their KEGG reactions; each with an SBO term.

**Changed**

- A reactant or product marked as a modifier of its own reaction (a
  self-regulation loop) now comes back through Antimony with the diagram.
- Opening an SBML file whose annotations hold another tool's own data (as
  COPASI writes) says that part won't be kept.

## 0.1.26 (2026-10-28)

**New**

- **Display names.** A species, reaction or compartment can have a display
  name besides its id, as SBML (`name`) and Antimony (`S1 is "glucose"`)
  allow: read from both, written back to both, and saved in PathwayDesigner's
  files (version 17). Each item's Properties show its id and its display
  name, which can be typed there. **View ▸ Display names** labels the diagram
  with names instead of ids, for models whose ids mean nothing, such as
  MINERVA's `species_11`; each item's **Label** list (As the diagram, Id,
  Display name) overrides it. The diagram language says the same with
  `labels displayName` and `{ label id }`.
- **Select All Drawings** (a species' right-click menu) selects a species and
  all its other drawings, so their look can be set at once.
- **Mark as Modifier / Unmark as Modifier** (a reaction's right-click menu)
  makes a reactant or product the rate law uses a modifier of the reaction
  too, as SBML allows (B in A -> B with `k*A*B`), drawn as a loop.
- **Gallery:** a positive-feedback example, next to negative feedback; the
  Millard *E. coli* model with each regulator drawn beside its reaction.

**Changed**

- **KEGG maps** keep KEGG's numbers as ids (`C00031`, `b1676`) and show the
  names, PubChem's for compounds (*D-Glucose*) and the box's label for
  enzymes (*pykF*), as display names; a map opens with display names shown.
- **Modifier symbols.** A modifier line stops where its symbol starts, and
  the symbol is drawn over the line's end, pointing along its last piece: an
  activator's arrowhead no longer has the line running through it and past
  its tip. A catalyst's circle keeps the same gap from the junction as the
  other symbols.
- In the Reaction properties, **Show Id** and **Id colour** are now **Show
  label** and **Label colour**, since the label can be the display name.
- An SBML file whose layout is incomplete (a layout without its size, as
  MINERVA writes for a hidden layer) is still refused, but the message now
  names what is missing and where.

## 0.1.25 (2026-10-27)

**New**

- **KEGG maps.** File ▸ Open reads a KEGG metabolic pathway map (KGML) at KEGG's
  positions: compounds as circles, enzymes as boxes catalysing their reactions,
  links to other maps as grey text, the map's title. Compounds are named from
  PubChem (`D_Glucose`, not `C00031`). Reactions whose compounds don't line up
  are drawn as rounded elbows, as KEGG draws them (KGML has no line routes).
  On the desktop, File ▸ Open from KEGG… fetches a map by its code
  (`eco00010`); in the browser it opens the map's file in a new tab to be saved
  and opened, since KEGG doesn't let web pages fetch its files. Signalling and
  global maps aren't read yet.
- **Help.** Help ▸ PathwayDesigner Help opens a panel beside the diagram, with
  pages on getting started, Antimony in PathwayDesigner and the diagram language.
- **Self-regulation loops.** A species that modifies a reaction it is also a
  reactant or product of (ATP and phosphofructokinase, the protons of a
  proton-driven reaction) is drawn as a loop from its own drawing back into the
  junction, instead of a line hidden under its own or crossing the map.
  Right-click a loop ▸ Convert Loop to Corner Points to shape it by hand.
- **Arrange ▸ Place Modifiers by Their Reactions.** Gives each modifier line
  from a far-off drawing its own drawing beside its reaction, and removes
  drawings left joined to nothing; for models imported with their effectors in
  a column at the edge. Works on the selected reactions, or all.
- **A fixed-width font** (Hack) for the Antimony text and the help's examples.

**Changed**

- Antimony export no longer writes Antimony's interaction lines (`X -| J1`); an
  activator's or inhibitor's kind goes in the diagram comment
  (`line X to J1 kind inhibitor`). Interaction lines are still read.
- The diagram format comes back as written: named colors (`color gold =
  #FFD54F`) are kept, a style's id side (`id right`) is kept, a second drawing
  (`ATP#2`) keeps its style, and a style keeps a size it gives. Named colors and
  sized styles are saved in PathwayDesigner's files as version 16.

## 0.1.24 (2026-10-25)

**New**

- **Defaults in the diagram format.** A diagram can state its defaults, kind by
  kind (`defaults species { fill #EEF6FF; … }`), and PathwayDesigner always
  writes them, so a file that gives little more than positions looks the same
  in any program that reads the format. A file's own defaults reach every item
  of their kind that doesn't say otherwise.
- **The Antimony panel** has *Diagram* and *Layout statements* beside *Load* and
  *Get*: Get Antimony writes the model with the drawing as a diagram, as
  Antimony's layout statements, both or neither.

**Changed**

- File ▸ Export Antimony with Layout Statements is a menu item of its own, in
  place of the checkbox in the File menu.
- Antimony export declares species four to a line.

## 0.1.23 (2026-10-25)

**New**

- **A readable diagram format.** File ▸ Export Antimony now writes the drawing
  after the model in a short, readable form meant to be read and edited by hand
  (`docs/diagram-format.md`; described in the technical report, Zenodo DOI
  10.5281/zenodo.23625331). It sits in a comment, `/* diagram … end */`, so
  the file is still plain Antimony that tellurium and other Antimony tools
  read. It has named styles, color names and constants, second drawings of a
  species (`ATP#2`), curves and corner points, modifier lines with their kinds,
  ids placed beside a node or at an offset, and the Escher theme. Opening such a
  file draws it as written, and lays out automatically any species it leaves
  out, keeping the others where they are. Every Gallery diagram comes back
  exactly through it.
- **Layout statements on request.** *Also write layout statements* in the File
  menu adds Antimony 3's own layout statements to the export, for tellurium and
  SBMLNetwork, which draw from those.

**Changed**

- The Antimony panel's *With layout* writes the diagram format.
- A selected reaction with a very thick line widens by at most 2, so it no
  longer outgrows its arrowheads.

## 0.1.22 (2026-10-24)

**New**

- **Antimony export with the drawing.** File ▸ Export Antimony writes the
  diagram's drawing as Antimony 3's layout statements: every species, alias,
  reaction junction and compartment box placed; every line with its ends and
  curve points; every look, by kind and then where items differ. libAntimony
  and PathwayDesigner read it back as drawn. What the statements can't say
  (smooth junctions, names outside nodes, dashes, saved styles and a few
  more) is listed after the export. The Antimony panel's Get Antimony writes
  the drawing too when *With layout* is ticked.

**Changed**

- Arrowheads on very thick reaction lines are at least twice the line's width
  across, so they stay visible.
- SBML export also writes the background on a global render information, so
  SBMLNetwork draws it.

**Fixed**

- Antimony export writes ids that libAntimony reserves (`compartment`, `time`,
  `pi`, `model`, …) with `_` added, so libAntimony and tellurium read the text.
  The diagram keeps its ids, and the export's notes list the renames.
- Thick curved reaction lines have round ends in the app, so they meet at the
  junction without a notch.

## 0.1.21 (2026-10-22)

**New**

- **Network structure.** A Structure tab shows the network's stoichiometry
  matrix, its conservation laws and a basis of its flux null space, computed
  by a QR factorisation with column pivoting. Choosing a law or a null-space
  vector draws it on the network: its species or reactions coloured by sign
  (blue positive, orange negative) and labelled with their coefficients, the
  rest faded. The basis can be shown as computed or made sparse, so that each
  vector involves as few reactions as it can. A panel below the diagram shows
  the stoichiometry matrix and the K and L matrices, and everything can be
  copied or exported as CSV. Checked against libRoadRunner on the Gallery and
  45 curated BioModels.
- **Corner points on reactant and product lines.** A reaction's lines, not only
  its modifier lines, can be routed through corner points: add, move and remove
  them on the canvas, with the curved piece at the species. They are saved, and
  written to SBML Layout as curves of several segments.
- **SBML Render.** A diagram's look (colours, line widths, dashes, shapes,
  fonts, arrowheads and modifier symbols) is written to and read from SBML with
  the Render package, so other tools such as COPASI and SBMLNetwork draw an
  exported file as PathwayDesigner does, and a file from them keeps its look.
- **Antimony layouts.** A model whose Antimony text describes its drawing with
  Antimony 3's layout statements (`S1.position = {…}`, aliases, lines,
  `S1.color = …`) opens drawn as the text says, as libAntimony reads it,
  colour names included. Auto-layout is then not offered.
- **Escher maps** keep their routed lines, and each drawing comes in at the size
  the map gives it.
- **Bold and italic names**, for species and reactions, also in the Escher theme.
- **Function definitions** in SBML and Antimony are written into the rate laws
  that call them, so a model with functions can be drawn, exported and
  simulated.

**Changed**

- In Antimony, the arrow says whether a reaction is reversible: `->` is
  reversible and `=>` irreversible. Constant species are written with `const`.
- Whether modifier lines are shown is saved with the diagram.
- A smooth junction keeps hand-shaped outer handles when things move.
- The matrix panel shows zeros, and the Structure tab's lists fold away.
- "Show name" on a reaction is now "Show Id", matching the Id field.
- The web version counts visits with Umami Cloud instead of the old visitor
  counter, which had started showing advertisements.

**Fixed**

- Antimony text with layout statements (`model.layout = on`) no longer fails
  to load or loses the values written after it.
- SBML: a hidden compartment is written without a box and comes back hidden;
  an alias listed before its species keeps its role and id; a participant drawn
  as an alias is no longer also made a modifier.
- An alias of a boundary species has its thick border.

**Gallery**

- The Millard et al. (2017) *E. coli* model, laid out after the published
  figure; a negative feedback example; Circle 4 removed (the same network as
  Circle 3).
