"""Checks SBML files with python-libsbml's checkConsistency() (SPEC-Rust
§8.3): no errors allowed, warnings reported. Also checks each file has a
layout with a glyph for every species. Exits with 1 on any error.

    python check_sbml.py <folder of .xml files>
"""
import os, sys, libsbml

folder = sys.argv[1]
bad = 0
files = sorted(f for f in os.listdir(folder) if f.endswith(".xml"))
for f in files:
    doc = libsbml.readSBMLFromFile(os.path.join(folder, f))
    doc.checkConsistency()
    errors = [doc.getError(i) for i in range(doc.getNumErrors())
              if doc.getError(i).getSeverity() >= libsbml.LIBSBML_SEV_ERROR]
    warnings = doc.getNumErrors() - len(errors)
    model = doc.getModel()
    layouts = model.getPlugin("layout").getNumLayouts() if model else 0
    glyphs = model.getPlugin("layout").getLayout(0).getNumSpeciesGlyphs() if layouts else 0
    if errors or layouts != 1 or glyphs < model.getNumSpecies():
        bad += 1
        print(f"{f}: {len(errors)} errors, {layouts} layouts, {glyphs} species glyphs")
        for e in errors[:5]:
            print("   ", e.getMessage().strip()[:300])
    else:
        print(f"{f}: ok ({warnings} warnings, {glyphs} species glyphs)")
print(f"{len(files) - bad} of {len(files)} files pass")
sys.exit(1 if bad or not files else 0)
