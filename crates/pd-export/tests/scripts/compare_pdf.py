"""Checks the PDFs written by `examples/export_models.rs` (SPEC-Rust §9):
each opens in pypdf with one page of the right size, and pypdfium2 (the PDF
engine of Chrome) draws each drawing-size PDF the same as resvg draws the PNG
of the same model. Exits with 1 on any failure.

    python compare_pdf.py <folder>
"""
import os, sys
import numpy as np
import pypdf, pypdfium2
from PIL import Image

def blur(a, r=1):
    """A (2r+1)-pixel box blur of an image array."""
    k = 2 * r + 1
    p = np.pad(a.astype(np.float64), ((r, r), (r, r), (0, 0)), mode="edge")
    c = np.pad(p.cumsum(0).cumsum(1), ((1, 0), (1, 0), (0, 0)))
    return (c[k:, k:] - c[:-k, k:] - c[k:, :-k] + c[:-k, :-k]) / (k * k)


folder = sys.argv[1]
names = sorted(f[:-4] for f in os.listdir(folder) if f.endswith(".png"))
bad = 0
for n in names:
    png = np.asarray(Image.open(os.path.join(folder, n + ".png")).convert("RGB"), dtype=np.int16)
    problems = []
    for suffix, size in [("", None), ("-letter", (612, 792))]:
        reader = pypdf.PdfReader(os.path.join(folder, n + suffix + ".pdf"), strict=True)
        box = reader.pages[0].mediabox
        if len(reader.pages) != 1 or (size and (round(float(box.width)), round(float(box.height))) != size):
            problems.append(f"{suffix or 'drawing'}: {len(reader.pages)} pages, {box}")
    # Drawing size: 0.75 pt per world unit; at 96/72 one pixel per world unit,
    # as in the PNG.
    page = pypdfium2.PdfDocument(os.path.join(folder, n + ".pdf"))[0]
    pdf = np.asarray(page.render(scale=96 / 72).to_pil().convert("RGB"), dtype=np.int16)
    h, w = min(png.shape[0], pdf.shape[0]), min(png.shape[1], pdf.shape[1])
    if abs(png.shape[0] - pdf.shape[0]) > 1 or abs(png.shape[1] - pdf.shape[1]) > 1:
        problems.append(f"size {pdf.shape[:2]} vs PNG {png.shape[:2]}")
    # The two engines anti-alias edges a little differently. Blurring both
    # by a pixel evens that out; a missing or misplaced shape still shows.
    diff = np.abs(blur(png[:h, :w]) - blur(pdf[:h, :w])).max(axis=2)
    worst = diff.max()
    if worst > 48:
        ys, xs = np.nonzero(diff > 48)
        problems.append(f"differs from the PNG by up to {worst:.0f} near ({xs[0]}, {ys[0]})")
    if problems:
        bad += 1
        print(f"{n}: " + "; ".join(problems))
    else:
        print(f"{n}: ok (largest difference {worst:.0f})")
print(f"{len(names) - bad} of {len(names)} models pass")
sys.exit(1 if bad or not names else 0)
