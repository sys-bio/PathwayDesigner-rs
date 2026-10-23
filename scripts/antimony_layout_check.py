"""Checks PathwayDesigner's reading and writing of Antimony's layout
statements against libAntimony (docs/antimony-layout-design.md §8,
docs/antimony-layout-export-design.md §8).

libAntimony can write the same layout two ways: as SBML Layout, and as
Antimony layout statements (`S1.position = {…}`). PathwayDesigner reads the
first with its SBML import and the second with its Antimony import; the two
drawings must agree. Three routes, over every Gallery model:

1. libAntimony's own layout: the model as Antimony with `model.layout = on`,
   laid out by libAntimony (SBMLNetwork) and written as Antimony.
2. PathwayDesigner's layout: its SBML export, written as Antimony by
   libAntimony.
3. PathwayDesigner's Antimony export (with its layout), read by libAntimony
   into SBML and compared with PathwayDesigner's own SBML export of the same
   model, glyph by glyph in the SBML itself: every species glyph's box,
   every reaction's centre, and the ends of every line's segments. (Read
   with `loadAntimonyString`: libAntimony's `loadAntimonyFile` fails on a
   large file it reads as a string.) Counted apart, as known: a second line
   between one reaction and one species, which Antimony can't name; a
   species drawing no line goes to (an alias is named by its reactions, so
   none is written); and libAntimony's own ways: it draws one line per whole
   unit of stoichiometry (an extra line beside the one written for 2, none
   for 0.5), and attaches an alias to the first reaction its name lists only
   (`S.position.J1.J2`: J2's line goes to S's main drawing).

PathwayDesigner reads the Antimony text, and its drawing is compared with
two references. Positions, sizes and which drawing each line goes to are
compared with libAntimony's own reading of the text (into SBML, read by
PathwayDesigner's SBML import): libAntimony's writer leaves out zero
positions and some sizes, and writes several drawings of a species for one
reaction under one name, so the text can say less than the SBML it came
from. Lines (handles, corner points, curved or not) are compared with the
SBML the text was written from: libAntimony's reading of its own lines is
unreliable (an end it left out comes back at (0, 0), or at another
reaction's place). A Linear reaction counts as straight: Linear is
PathwayDesigner's own annotation, which Antimony doesn't carry.

Looks (each item's colours, widths, name and shape, the background) are
compared with libAntimony's reading on route 2 only: on route 1 libAntimony
leaves its own default look out of the text, so its SBML is drawn in
SBMLNetwork's colours and PathwayDesigner's reading in its own defaults, by
design.

Known difference: libAntimony 3.2.1 attaches an alias (`S1.position.J2.J3`)
to the first reaction listed only; the reference, and PathwayDesigner,
attach it to every reaction listed. The yeast glycolysis model shows it.

Compares every node's centre and size, which species each alias draws, every
reaction's junction and whether it is drawn curved, the nodes its lines go
to, each line's handles and corner points, and compartment boxes.

    python scripts/antimony_layout_check.py [<work folder>]

Each model's libAntimony work runs in a child process with a time limit
(libAntimony's automatic layout can take very long on a large model); a
model that runs out of time is reported and skipped.

Needs the `antimony` Python package (libAntimony 3.2.1 was used), and builds
the `export_sbml` and `drawing_report` examples.
"""

import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOL = 0.01  # libAntimony writes positions with two decimals
LIMIT = 120  # seconds of libAntimony per model


def cargo_example(name, *args):
    out = subprocess.run(
        ["cargo", "run", "-q", "--release", "-p", "pd-core", "--example", name, "--", *args],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    if out.returncode != 0:
        raise RuntimeError(out.stderr[-2000:])
    return out.stdout


def report(path):
    return json.loads(cargo_example("drawing_report", path))


def norm(d):
    """A drawing keyed by what doesn't depend on made-up ids."""
    nodes = {n["id"]: n for n in d["nodes"]}

    def name(node_id):
        n = nodes.get(node_id)
        if n is None:
            return node_id
        return "null" if n["null"] else n["of"]

    species = {}
    for n in d["nodes"]:
        if not n["null"]:
            species.setdefault(n["of"], []).append(n)
    reactions = {}
    for r in d["reactions"]:
        ends = []
        for role in ("reactants", "products", "modifiers"):
            for e in r[role]:
                n = nodes.get(e)
                at = None if n is None else (round(n["x"], 1), round(n["y"], 1))
                ends.append((role, name(e), at))
        lines = sorted(
            ((l["role"], name(l["node"]), l["handles"], l["points"]) for l in r.get("lines", [])),
            key=lambda t: (t[0], t[1], str(t[2]), str(t[3])))
        reactions[r["id"]] = {"x": r["x"], "y": r["y"], "ends": sorted(ends, key=str),
                              "mode": r.get("mode"), "lines": lines}
    nulls = sorted((round(n["x"], 1), round(n["y"], 1), n["w"], n["h"])
                   for n in d["nodes"] if n["null"])
    comps = {c["id"]: c for c in d["compartments"]}
    return species, reactions, nulls, comps


def close(a, b, tol=TOL):
    return abs(a - b) <= tol + 1e-9


def compare(label, a, b, lines_ref, looks=False):
    """Differences between drawing b (from Antimony) and the references: a
    for places, lines_ref for lines (both read from SBML)."""
    sa, ra, na, ca = norm(a)
    sb, rb, nb, cb = norm(b)
    _, rl, _, _ = norm(lines_ref)
    out = []
    moved = 0
    for sid in sorted(set(sa) | set(sb)):
        xa = sorted((n["x"], n["y"], n["w"], n["h"]) for n in sa.get(sid, []))
        xb = sorted((n["x"], n["y"], n["w"], n["h"]) for n in sb.get(sid, []))
        if len(xa) != len(xb) or any(not all(close(p, q) for p, q in zip(u, v))
                                     for u, v in zip(xa, xb)):
            out.append(f"species {sid}: SBML {xa} / Antimony {xb}")
    for rid in sorted(set(ra) | set(rb)):
        if rid not in ra or rid not in rb:
            out.append(f"reaction {rid} only in one")
            continue
        p, q = ra[rid], rb[rid]
        if not (close(p["x"], q["x"]) and close(p["y"], q["y"])):
            out.append(f"reaction {rid} junction: SBML ({p['x']:.2f}, {p['y']:.2f}) / "
                       f"Antimony ({q['x']:.2f}, {q['y']:.2f})")
        ea, eb = p["ends"], q["ends"]
        same = len(ea) == len(eb) and all(
            u[0] == v[0] and u[1] == v[1] and (u[2] is None) == (v[2] is None)
            and (u[2] is None or (close(u[2][0], v[2][0], 0.11) and close(u[2][1], v[2][1], 0.11)))
            for u, v in zip(ea, eb))
        if not same:
            out.append(f"reaction {rid} lines: SBML {ea} / Antimony {eb}")
        lp = rl.get(rid, p)
        # libAntimony's writer drops a reaction box's zero size, which then
        # means 20 x 20, so in its own reading the junction moves by (10, 10);
        # such a reaction's lines can't match the SBML they came from.
        if not (close(lp["x"], p["x"]) and close(lp["y"], p["y"])):
            moved += 1
            continue
        straight = lambda m: "Straight" if m == "Linear" else m
        if straight(lp["mode"]) != straight(q["mode"]):
            out.append(f"reaction {rid} mode: SBML {lp['mode']} / Antimony {q['mode']}")
        # Lines paired by role and species; a line only one side has (SBML
        # import keeps a product that is also a modifier, Antimony import
        # doesn't) is a difference of the models, not of the layout.
        theirs = {}
        for v in q["lines"]:
            theirs.setdefault((v[0], v[1]), []).append(v)
        pairs = []
        for u in lp["lines"]:
            if theirs.get((u[0], u[1])):
                pairs.append((u, theirs[(u[0], u[1])].pop(0)))
        for u, v in pairs:
            hu, hv = u[2], v[2]
            handles_same = (hu is None) == (hv is None) and (
                hu is None or all(close(a_, b_, 0.05) for a_, b_ in zip(hu, hv)))
            points_same = len(u[3]) == len(v[3]) and all(
                close(a_[0], b_[0], 0.05) and close(a_[1], b_[1], 0.05) for a_, b_ in zip(u[3], v[3]))
            if not (handles_same and points_same):
                out.append(f"reaction {rid} {u[0]} line to {u[1]}: SBML handles {hu} points {u[3]} / "
                           f"Antimony handles {hv} points {v[3]}")
    if len(na) != len(nb) or any(not (close(u[0], v[0], 0.11) and close(u[1], v[1], 0.11))
                                 for u, v in zip(na, nb)):
        out.append(f"null nodes: SBML {na} / Antimony {nb}")
    for cid in sorted(set(ca) | set(cb)):
        u, v = ca.get(cid), cb.get(cid)
        if u is None or v is None or not all(close(u[k], v[k]) for k in ("x", "y", "w", "h")):
            out.append(f"compartment {cid}: SBML {u} / Antimony {v}")
    if looks:
        def by_name(d):
            out = {}
            for n in d["nodes"]:
                if not n["null"]:
                    out.setdefault(n["of"], []).append(n.get("look"))
            return {k: sorted(v) for k, v in out.items()}
        la, lb = by_name(a), by_name(b)
        for sid in sorted(set(la) & set(lb)):
            if la[sid] != lb[sid]:
                out.append(f"species {sid} look: SBML {la[sid]} / Antimony {lb[sid]}")
        ra_ = {r["id"]: r.get("look") for r in a["reactions"]}
        rb_ = {r["id"]: r.get("look") for r in b["reactions"]}
        for rid in sorted(set(ra_) & set(rb_)):
            if ra_[rid] != rb_[rid]:
                out.append(f"reaction {rid} look: SBML {ra_[rid]} / Antimony {rb_[rid]}")
        ca_ = {c["id"]: c.get("look") for c in a["compartments"]}
        cb_ = {c["id"]: c.get("look") for c in b["compartments"]}
        for cid in sorted(set(ca_) & set(cb_)):
            if ca_[cid] != cb_[cid]:
                out.append(f"compartment {cid} look: SBML {ca_[cid]} / Antimony {cb_[cid]}")
        if a.get("background") != b.get("background"):
            out.append(f"background: SBML {a.get('background')} / Antimony {b.get('background')}")
    note = f" (lines of {moved} reactions not compared: libAntimony moved their junction)" if moved else ""
    print(f"  {label}: {'agree' if not out else str(len(out)) + ' differences'}{note}", flush=True)
    for line in out[:8]:
        print("    " + line, flush=True)
    return len(out)


def geometry(path):
    """An SBML layout's glyphs: species boxes by species, reaction centres,
    and each line's segment ends (direction left out), keyed by reaction,
    species ("null" for a null node) and whether it is a modifier's."""
    import libsbml
    doc = libsbml.readSBMLFromFile(path)
    model = doc.getModel()
    lay = model.getPlugin("layout").getLayout(0)
    boxes, centres, lines = {}, {}, {}
    # Stoichiometry by (reaction, species), for libAntimony's one line per unit.
    stoich = {}
    for r in model.getListOfReactions():
        for ref in list(r.getListOfReactants()) + list(r.getListOfProducts()):
            stoich[(r.getId(), ref.getSpecies())] = ref.getStoichiometry()
    names = {}
    for g in lay.getListOfSpeciesGlyphs():
        sp = g.getSpeciesId() or "null"
        names[g.getId()] = sp
        b = g.getBoundingBox()
        boxes.setdefault(sp, []).append((b.getX(), b.getY(), b.getWidth(), b.getHeight()))
    for r in lay.getListOfReactionGlyphs():
        b = r.getBoundingBox()
        centres[r.getReactionId()] = (b.getX() + b.getWidth() / 2, b.getY() + b.getHeight() / 2)
        for sr in r.getListOfSpeciesReferenceGlyphs():
            role = "modifier" if sr.getRoleString() in (
                "modifier", "activator", "inhibitor", "sidesubstrate", "sideproduct") else "leg"
            ends = []
            for seg in sr.getCurve().getListOfCurveSegments():
                a, e = seg.getStart(), seg.getEnd()
                # Rounded as written (two decimals), so the order of near-equal
                # ends doesn't depend on the last digits.
                r2 = lambda x: round(x, 2)
                ends.append(sorted([(r2(a.getXOffset()), r2(a.getYOffset())),
                                    (r2(e.getXOffset()), r2(e.getYOffset()))]))
            key = (r.getReactionId(), names.get(sr.getSpeciesGlyphId()), role)
            lines.setdefault(key, []).append(sorted(ends))
    return boxes, centres, lines, stoich


def compare_sbml(ours, theirs):
    """Route 3: PathwayDesigner's SBML against libAntimony's reading of its
    Antimony export, in the SBML itself."""
    tol = 0.02  # two decimals
    near = lambda p, q: abs(p[0] - q[0]) <= tol and abs(p[1] - q[1]) <= tol
    ba, ca, la, stoich = geometry(ours)
    bb, cb, lb, _ = geometry(theirs)
    out, unnamed, unconnected, extra, fractional, alias_fault = [], 0, 0, 0, 0, 0
    # Reactions listed after the first in an alias's name, by species.
    later = set()
    for line in open(ours[:-4] + ".ant", encoding="utf-8"):
        parts = line.strip().split(" = ")[0].split(".")
        if len(parts) >= 4 and parts[1] == "position":
            for r in parts[3:]:
                later.add((r, parts[0]))
    below_one = lambda key: stoich.get(key[:2], 1.0) < 1.0
    # Where our lines libAntimony can draw end, to tell a drawing no line
    # goes to.
    line_ends = [pt for key, curves in la.items() if not below_one(key)
                 for c in curves for seg in c for pt in seg]

    def connected(box):
        x, y, w, h = box
        return any(x - 1 <= px <= x + w + 1 and y - 1 <= py <= y + h + 1 for px, py in line_ends)

    same_box = lambda p, q: all(abs(u - v) <= tol for u, v in zip(p, q))
    for sp in sorted(set(ba) - {"null"}):
        xb = list(bb.get(sp, []))
        for box in sorted(ba[sp]):
            match = next((q for q in xb if same_box(box, q)), None)
            if match is not None:
                xb.remove(match)
            elif not connected(box):
                unconnected += 1
            else:
                out.append(f"species {sp}: ours {box} not drawn by libAntimony")
        for q in xb:
            out.append(f"species {sp}: libAntimony draws {q}, which ours doesn't")
    for rid, c in ca.items():
        if rid not in cb or not near(c, cb[rid]):
            out.append(f"reaction {rid} centre: ours {c} / libAntimony {cb.get(rid)}")
    # Two lines of one reaction to one species: Antimony names only one.
    pairs = {}
    for (rid, sp, _), curves in la.items():
        pairs[(rid, sp)] = pairs.get((rid, sp), 0) + len(curves)
    for key, curves in la.items():
        if pairs[key[:2]] > 1 and key[1] != "null":
            unnamed += len(curves)
            continue
        theirs_curves = lb.get(key)
        if theirs_curves is None:
            if below_one(key):
                fractional += len(curves)
            else:
                out.append(f"line {key}: not drawn by libAntimony")
            continue
        same = lambda u, v: len(u) == len(v) and all(
            near(p[0], q[0]) and near(p[1], q[1]) for p, q in zip(u, v))
        left = list(theirs_curves)
        for u in curves:
            match = next((v for v in left if same(u, v)), None)
            if match is None:
                if key[:2] in later:
                    alias_fault += 1
                else:
                    out.append(f"line {key}: ours {u} / libAntimony {left}")
            else:
                left.remove(match)
        extra += len(left)
    known = [f"{n} {what}" for n, what in [
        (unnamed, "second lines to one species, which Antimony can't name"),
        (unconnected, "drawings no line goes to, not written"),
        (extra, "extra lines libAntimony adds for stoichiometry above one"),
        (fractional, "lines with stoichiometry below one, which libAntimony doesn't draw"),
        (alias_fault, "lines libAntimony moves to the main drawing (its alias fault)"),
    ] if n]
    note = f" (known: {'; '.join(known)})" if known else ""
    print(f"  PathwayDesigner's export: {'agree' if not out else str(len(out)) + ' differences'}{note}",
          flush=True)
    for line in out[:8]:
        print("    " + line, flush=True)
    return len(out)


def one(ours, work):
    """libAntimony's part for one model: writes the route files."""
    import antimony
    stem = os.path.basename(ours)[:-4]
    # Route 3 first: it needs no automatic layout, so it is quick.
    antimony.clearPreviousLoads()
    if antimony.loadAntimonyString(open(ours[:-4] + ".ant", encoding="utf-8").read()) < 0:
        print("libAntimony can't read PathwayDesigner's Antimony:", antimony.getLastError()[:200])
        return 1
    with open(os.path.join(work, stem + ".route3.xml"), "w", encoding="utf-8") as fh:
        fh.write(antimony.getSBMLString(antimony.getMainModuleName()))
    antimony.clearPreviousLoads()
    if antimony.loadSBMLFile(ours) < 0:
        print("libAntimony can't read it:", antimony.getLastError()[:200])
        return 1
    plain = antimony.getAntimonyString(antimony.getMainModuleName())
    # Route 2: PathwayDesigner's layout, as libAntimony writes it in Antimony.
    with open(os.path.join(work, stem + ".route2.ant"), "w", encoding="utf-8") as fh:
        fh.write(plain)
    reread(antimony, plain, os.path.join(work, stem + ".route2.xml"))
    # Route 1: the model alone, laid out by libAntimony.
    # libAntimony writes the layout last, before the model's `end`.
    # `model.layout = on` goes inside the model, before its `end` (a line
    # such as `m is "m"` may follow it).
    lines = plain.splitlines()
    ends = [i for i, l in enumerate(lines) if l.strip() == "end"]
    first = next((i for i, l in enumerate(lines) if l.strip().startswith("model.layout")), None)
    if first is not None:
        lines = lines[:first] + lines[ends[-1]:]
        ends = [i for i, l in enumerate(lines) if l.strip() == "end"]
    at = ends[-1] if ends else len(lines)
    model_only = "\n".join(lines[:at] + ["model.layout = on"] + lines[at:]) + "\n"
    antimony.clearPreviousLoads()
    if antimony.loadAntimonyString(model_only) < 0:
        print("libAntimony can't lay it out:", antimony.getLastError()[:200])
        return 1
    sb1 = os.path.join(work, stem + ".route1.xml")
    with open(sb1, "w", encoding="utf-8") as fh:
        fh.write(antimony.getSBMLString(antimony.getMainModuleName()))
    antimony.clearPreviousLoads()
    antimony.loadSBMLFile(sb1)
    text1 = antimony.getAntimonyString(antimony.getMainModuleName())
    with open(os.path.join(work, stem + ".route1.ant"), "w", encoding="utf-8") as fh:
        fh.write(text1)
    reread(antimony, text1, os.path.join(work, stem + ".route1.read.xml"))
    return 0


def reread(antimony, text, path):
    """libAntimony's reading of Antimony `text`, as SBML. SBMLNetwork splits
    off a drawing of any species with more than maxNumConnectedEdges lines
    (default 3), even one the text already places; the text says nothing of
    such drawings, so that is turned off."""
    antimony.clearPreviousLoads()
    antimony.loadAntimonyString(text.replace(
        "model.layout = on", "model.layout = on\nmodel.autolayout.maxNumConnectedEdges = 10000", 1))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(antimony.getSBMLString(antimony.getMainModuleName()))


def main():
    work = sys.argv[1] if len(sys.argv) > 1 else tempfile.mkdtemp(prefix="antlayout_")
    os.makedirs(work, exist_ok=True)
    sbml_dir = os.path.join(work, "gallery_sbml")
    cargo_example("export_sbml", sbml_dir, "--gallery", "--antimony")
    failures = 0
    for f in sorted(os.listdir(sbml_dir)):
        if not f.endswith(".xml"):
            continue
        stem = f[:-4]
        ours = os.path.join(sbml_dir, f)
        print(stem, flush=True)
        sb3 = os.path.join(work, stem + ".route3.xml")
        if os.path.exists(sb3):
            os.remove(sb3)
        try:
            child = subprocess.run([sys.executable, __file__, "--one", ours, work],
                                   capture_output=True, text=True, timeout=LIMIT)
        except subprocess.TimeoutExpired:
            print(f"  routes 1 and 2 skipped: libAntimony took more than {LIMIT} s", flush=True)
            if os.path.exists(sb3):
                failures += compare_sbml(ours, sb3) > 0
            continue
        if child.returncode != 0:
            print("  " + (child.stdout + child.stderr).strip()[-300:], flush=True)
            failures += 1
            continue
        sb1 = os.path.join(work, stem + ".route1.read.xml")
        src1 = os.path.join(work, stem + ".route1.xml")
        ant1 = os.path.join(work, stem + ".route1.ant")
        sb2 = os.path.join(work, stem + ".route2.xml")
        ant2 = os.path.join(work, stem + ".route2.ant")
        # A route without a layout would agree trivially (both readers place
        # at random with the same seed): that is a failure of the check.
        empty = [p for p in (ant1, ant2) if ".position" not in open(p, encoding="utf-8").read()]
        if empty:
            print("  no layout statements in " + ", ".join(os.path.basename(p) for p in empty),
                  flush=True)
            failures += 1
            continue
        failures += compare_sbml(ours, sb3) > 0
        failures += compare("libAntimony's layout", report(sb1), report(ant1), report(src1)) > 0
        failures += compare("PathwayDesigner's layout", report(sb2), report(ant2), report(ours),
                            looks=True) > 0
    print(f"{failures} comparisons with differences")
    return 1 if failures else 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--one":
        sys.exit(one(sys.argv[2], sys.argv[3]))
    sys.exit(main())
