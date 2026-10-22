"""Checks PathwayDesigner's reading of Antimony's layout statements against
libAntimony (docs/antimony-layout-design.md §8).

libAntimony can write the same layout two ways: as SBML Layout, and as
Antimony layout statements (`S1.position = {…}`). PathwayDesigner reads the
first with its SBML import and the second with its Antimony import; the two
drawings must agree. Two routes, over every Gallery model:

1. libAntimony's own layout: the model as Antimony with `model.layout = on`,
   laid out by libAntimony (SBMLNetwork) and written as Antimony.
2. PathwayDesigner's layout: its SBML export, written as Antimony by
   libAntimony.

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


def compare(label, a, b, lines_ref):
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
    note = f" (lines of {moved} reactions not compared: libAntimony moved their junction)" if moved else ""
    print(f"  {label}: {'agree' if not out else str(len(out)) + ' differences'}{note}", flush=True)
    for line in out[:8]:
        print("    " + line, flush=True)
    return len(out)


def one(ours, work):
    """libAntimony's part for one model: writes the route files."""
    import antimony
    stem = os.path.basename(ours)[:-4]
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
    cargo_example("export_sbml", sbml_dir, "--gallery")
    failures = 0
    for f in sorted(os.listdir(sbml_dir)):
        if not f.endswith(".xml"):
            continue
        stem = f[:-4]
        ours = os.path.join(sbml_dir, f)
        print(stem, flush=True)
        try:
            child = subprocess.run([sys.executable, __file__, "--one", ours, work],
                                   capture_output=True, text=True, timeout=LIMIT)
        except subprocess.TimeoutExpired:
            print(f"  skipped: libAntimony took more than {LIMIT} s", flush=True)
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
        failures += compare("libAntimony's layout", report(sb1), report(ant1), report(src1)) > 0
        failures += compare("PathwayDesigner's layout", report(sb2), report(ant2), report(ours)) > 0
    print(f"{failures} comparisons with differences")
    return 1 if failures else 0


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--one":
        sys.exit(one(sys.argv[2], sys.argv[3]))
    sys.exit(main())
