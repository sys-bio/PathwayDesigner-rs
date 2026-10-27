# Release notes

What changed in each version, for the GitHub release. The packages for
Windows, macOS and Linux and the web zip are attached to each release; the web
version at https://sys-bio.github.io/PathwayDesigner-rs/ is always the latest.

## Next version (not yet released)

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
