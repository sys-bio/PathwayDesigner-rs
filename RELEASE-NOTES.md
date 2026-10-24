# Release notes

What changed in each version, for the GitHub release. The packages for
Windows, macOS and Linux and the web zip are attached to each release; the web
version at https://sys-bio.github.io/PathwayDesigner-rs/ is always the latest.

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
