"""Checks curated BioModels after a round trip through PathwayDesigner.

    cargo run -p pd-core --release --example biomodels_roundtrip -- <BioModels> <out>
    python crates/pd-core/tests/scripts/biomodels_check.py <BioModels> <out> <report.md>

For each model the round trip wrote, it compares the original SBML with the
round-tripped file:

- structure, with libSBML: the same species, reactions (and their
  stoichiometry), compartments, global parameters and assignment rules, and
  how many annotation terms (CV terms) and SBO terms they carry;
- behaviour, with libRoadRunner: both simulated from 0 to 100 with tight
  solver tolerances (relative 1e-10, absolute 1e-12), and every floating
  species' concentration compared at 201 times. The largest relative
  difference is recorded.

Models whose import said it left something out (what Level 2 says that
Level 3 can't, CellDesigner's and JDesigner's own annotations) are reported
apart. Each
model is checked in a process of its own, with a time limit, so a model that
makes the simulator hang can't stall the run. Writes the report as Markdown,
and every model's result as JSON next to it.
"""

import concurrent.futures as cf
import glob
import json
import os
import subprocess
import sys

TIMEOUT = 180
END, POINTS = 100.0, 201
REL, ABS = 1e-10, 1e-12
# A relative difference this small is solver round-off, not a change in the
# model (the round trip may write an expression in another, equal form).
SAME = 1e-6


def original_file(src, mid):
    folder = os.path.join(src, mid)
    url = os.path.join(folder, f"{mid}_url.xml")
    if os.path.exists(url):
        return url
    for f in sorted(glob.glob(os.path.join(folder, "*.xml"))):
        name = os.path.basename(f)
        if name == "manifest.xml" or name.endswith("_urn.xml"):
            continue
        with open(f, "rb") as fh:
            if b"<sbml" in fh.read(2000):
                return f
    path = os.path.join(src, f"{mid}.xml")
    return path if os.path.exists(path) else None


def structure(path):
    import libsbml

    doc = libsbml.readSBMLFromFile(path)
    m = doc.getModel()
    if m is None:
        return None

    def cv(sb):
        terms = sb.getCVTerms()
        n = 0
        if terms:
            for t in terms:
                n += t.getNumResources()
        return n

    items = (
        [m]
        + [m.getSpecies(i) for i in range(m.getNumSpecies())]
        + [m.getReaction(i) for i in range(m.getNumReactions())]
        + [m.getCompartment(i) for i in range(m.getNumCompartments())]
        + [m.getParameter(i) for i in range(m.getNumParameters())]
    )
    reactions = {}
    for i in range(m.getNumReactions()):
        r = m.getReaction(i)
        st = {}
        for j in range(r.getNumReactants()):
            s = r.getReactant(j)
            st[s.getSpecies()] = st.get(s.getSpecies(), 0) - s.getStoichiometry()
        for j in range(r.getNumProducts()):
            s = r.getProduct(j)
            st[s.getSpecies()] = st.get(s.getSpecies(), 0) + s.getStoichiometry()
        reactions[r.getId()] = st
    rules = [m.getRule(i) for i in range(m.getNumRules())]
    return {
        "species": sorted(m.getSpecies(i).getId() for i in range(m.getNumSpecies())),
        "reactions": reactions,
        "compartments": sorted(m.getCompartment(i).getId() for i in range(m.getNumCompartments())),
        "parameters": sorted(m.getParameter(i).getId() for i in range(m.getNumParameters())),
        "assignment_rules": sorted(r.getVariable() for r in rules if r.isAssignment()),
        "other_rules": sum(1 for r in rules if not r.isAssignment()),
        "events": m.getNumEvents(),
        "constraints": m.getNumConstraints(),
        "functions": m.getNumFunctionDefinitions(),
        "cv_terms": sum(cv(x) for x in items),
        "sbo_terms": sum(1 for x in items if x.isSetSBOTerm()),
    }


def simulate(path, tight=True):
    import roadrunner

    rr = roadrunner.RoadRunner(path)
    if tight:
        rr.integrator.relative_tolerance = REL
        rr.integrator.absolute_tolerance = ABS
    ids = list(rr.model.getFloatingSpeciesIds())
    rr.timeCourseSelections = ["time"] + [f"[{i}]" for i in ids]
    res = rr.simulate(0, END, POINTS)
    return ids, [list(map(float, row)) for row in res]


def compare(a, b):
    """The largest relative difference between two simulations, by species
    id; and the species the round trip lost."""
    ids_a, rows_a = a
    ids_b, rows_b = b
    worst, where = 0.0, None
    missing = [i for i in ids_a if i not in ids_b]
    for i, sid in enumerate(ids_a):
        if sid not in ids_b:
            continue
        j = ids_b.index(sid)
        col_a = [r[i + 1] for r in rows_a]
        col_b = [r[j + 1] for r in rows_b]
        scale = max(max(abs(x) for x in col_a), max(abs(x) for x in col_b), 1e-300)
        for x, y in zip(col_a, col_b):
            d = abs(x - y) / max(abs(x), abs(y), scale * 1e-9, 1e-15)
            if d != d:  # NaN in one only
                d = 0.0 if (x != x and y != y) else float("inf")
            if d > worst:
                worst, where = d, sid
    return worst, where, missing


def check_one(original, roundtrip):
    out = {}
    try:
        out["before"] = structure(original)
        out["after"] = structure(roundtrip)
    except Exception as e:  # noqa: BLE001
        out["structure_error"] = str(e)
    # Tight tolerances first; a stiff model the solver can't take that
    # far is simulated, both files alike, at libRoadRunner's defaults.
    a = None
    for tight in (True, False):
        try:
            a = simulate(original, tight)
            out["tight"] = tight
            break
        except Exception as e:  # noqa: BLE001
            error = str(e).splitlines()[0][:200] if str(e) else type(e).__name__
    if a is None:
        out["sim"] = "original does not simulate"
        out["sim_error"] = error
        return out
    try:
        b = simulate(roundtrip, out["tight"])
    except Exception as e:  # noqa: BLE001
        out["sim"] = "round trip does not simulate"
        out["sim_error"] = str(e).splitlines()[0][:200]
        return out
    worst, where, missing = compare(a, b)
    out["sim"] = "ok"
    out["max_rel_diff"] = worst
    out["worst_species"] = where
    out["missing_species"] = missing
    return out


def run_one(args):
    mid, original, roundtrip = args
    try:
        p = subprocess.run(
            [sys.executable, __file__, "--one", original, roundtrip],
            capture_output=True,
            text=True,
            timeout=TIMEOUT,
        )
        found = [l for l in p.stdout.splitlines() if l.startswith("RESULT ")]
        result = (
            json.loads(found[-1][len("RESULT "):])
            if found
            else {"sim": "crashed", "sim_error": (p.stderr or p.stdout)[-300:]}
        )
    except subprocess.TimeoutExpired:
        result = {"sim": f"no answer within {TIMEOUT} s"}
    result["id"] = mid
    return result


def structural_differences(before, after):
    """What the round trip changed in the model's structure, as phrases."""
    out = []
    for key in ("species", "compartments", "assignment_rules"):
        lost = sorted(set(before[key]) - set(after[key]))
        if lost:
            out.append(f"{key.replace('_', ' ')} lost: {', '.join(lost[:5])}")
    lost = sorted(set(before["parameters"]) - set(after["parameters"]) - set(after["assignment_rules"]))
    if lost:
        out.append(f"parameters lost: {', '.join(lost[:5])}")
    lost = sorted(set(before["reactions"]) - set(after["reactions"]))
    if lost:
        out.append(f"reactions lost: {', '.join(lost[:5])}")
    changed = [
        r
        for r, st in before["reactions"].items()
        if r in after["reactions"] and {k: v for k, v in st.items() if v} != {k: v for k, v in after["reactions"][r].items() if v}
    ]
    if changed:
        out.append(f"stoichiometry changed: {', '.join(changed[:5])}")
    if after["cv_terms"] < before["cv_terms"]:
        out.append(f"annotation terms {before['cv_terms']} → {after['cv_terms']}")
    if after["sbo_terms"] < before["sbo_terms"]:
        out.append(f"SBO terms {before['sbo_terms']} → {after['sbo_terms']}")
    return out


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    if sys.argv[1] == "--one":
        # Marked: libRoadRunner writes its warnings to the same output.
        print("RESULT " + json.dumps(check_one(sys.argv[2], sys.argv[3])), flush=True)
        return
    src, out_dir, report = sys.argv[1:4]
    log = {}
    with open(os.path.join(out_dir, "roundtrip.jsonl"), encoding="utf-8") as fh:
        for line in fh:
            r = json.loads(line)
            log[r["id"]] = r
    jobs = []
    for mid, r in sorted(log.items()):
        if r["read"]:
            # The file the round trip read, where a folder has several.
            original = log[mid].get("file") or original_file(src, mid)
            jobs.append((mid, original, os.path.join(out_dir, f"{mid}.xml")))
    results = []
    with cf.ThreadPoolExecutor(max_workers=max(2, (os.cpu_count() or 4) - 2)) as pool:
        for i, res in enumerate(pool.map(run_one, jobs), 1):
            results.append(res)
            if i % 50 == 0:
                print(f"{i} of {len(jobs)} checked", flush=True)
    with open(report.replace(".md", ".json"), "w", encoding="utf-8") as fh:
        json.dump(results, fh, indent=1)
    write_report(report, log, results)


def write_report(path, log, results):
    def dropped(r):
        # What the import said it doesn't keep (docs/sbml-full-support-design.md:
        # only Level 2's parts Level 3 has no place for, and CellDesigner's and
        # JDesigner's own annotations).
        said = log.get(r["id"], {}).get("warnings", [])
        return [w for w in said if "aren't kept" in w or "have no place" in w or "left out" in w]

    groups = {"same": [], "same_dropped": [], "differs": [], "differs_dropped": [], "original": [], "roundtrip": [], "other": []}
    for r in results:
        sim = r.get("sim")
        d = dropped(r)
        if sim == "ok":
            close = r["max_rel_diff"] <= SAME and not r.get("missing_species")
            key = ("same" if close else "differs") + ("_dropped" if d else "")
        elif sim == "original does not simulate":
            key = "original"
        elif sim == "round trip does not simulate":
            key = "roundtrip"
        else:
            key = "other"
        groups[key].append(r)
    diffs = sorted(r["max_rel_diff"] for r in results if r.get("sim") == "ok")
    n = len(results)
    lines = [
        "# Curated BioModels through PathwayDesigner and back",
        "",
        f"{len(log)} curated models found; {sum(1 for r in log.values() if r['read'])} read by PathwayDesigner "
        f"and written back as SBML. Each pair simulated with libRoadRunner from 0 to {END:g} "
        f"({POINTS} times, relative tolerance {REL:g}, absolute {ABS:g}, or libRoadRunner's defaults "
        f"for the {sum(1 for r in results if r.get('tight') is False)} stiff models it can't simulate so "
        f"closely, both files alike); species concentrations compared.",
        "",
        "| Result | Models |",
        "|---|---|",
        f"| Same behaviour (largest relative difference ≤ {SAME:g}) | {len(groups['same'])} |",
        f"| Same behaviour, though it has parts PathwayDesigner leaves out | {len(groups['same_dropped'])} |",
        f"| Different, and it has parts PathwayDesigner leaves out (expected) | {len(groups['differs_dropped'])} |",
        f"| **Different, with nothing left out** | **{len(groups['differs'])}** |",
        f"| The original doesn't simulate in libRoadRunner | {len(groups['original'])} |",
        f"| **The round trip doesn't simulate, though the original does** | **{len(groups['roundtrip'])}** |",
        f"| No answer in time, or the check crashed | {len(groups['other'])} |",
        f"| Total | {n} |",
        "",
    ]
    if diffs:
        same = [r["max_rel_diff"] for r in groups["same"]]
        exact = sum(1 for d in same if d == 0.0)
        lines += [
            f"Of the {len(same)} with the same behaviour, {exact} simulate bit for bit the same; "
            f"the largest difference among the rest is {max(same) if same else 0:.1e}.",
            "",
        ]

    def table(title, rows, cols):
        if not rows:
            return
        lines.extend([f"## {title}", "", "| Model | " + " | ".join(c[0] for c in cols) + " |", "|---" * (len(cols) + 1) + "|"])
        for r in rows:
            lines.append(f"| {r['id']} | " + " | ".join(str(c[1](r)) for c in cols) + " |")
        lines.append("")

    struct = lambda r: "; ".join(structural_differences(r["before"], r["after"])) if r.get("before") and r.get("after") else ""
    warn = lambda r: "; ".join(log[r["id"]].get("warnings", [])[:3])[:200]
    table(
        "Different, with nothing left out",
        sorted(groups["differs"], key=lambda r: -r["max_rel_diff"]),
        [("Largest difference", lambda r: f"{r['max_rel_diff']:.2e} ({r.get('worst_species')})"), ("Structure", struct), ("Import said", warn)],
    )
    table(
        "The round trip doesn't simulate",
        groups["roundtrip"],
        [("Error", lambda r: r.get("sim_error", "")), ("Import said", warn)],
    )
    table(
        "Different, with parts left out (expected)",
        groups["differs_dropped"],
        [("Left out", lambda r: ", ".join(dropped(r))), ("Largest difference", lambda r: f"{r['max_rel_diff']:.1e}")],
    )
    table("No answer in time, or crashed", groups["other"], [("Result", lambda r: r.get("sim", ""))])
    changed = [r for r in groups["same"] + groups["same_dropped"] if r.get("before") and r.get("after") and structural_differences(r["before"], r["after"])]
    table("Same behaviour, but the structure changed", changed, [("Structure", struct)])
    table("The original doesn't simulate", groups["original"], [("libRoadRunner said", lambda r: r.get("sim_error", ""))])
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines))
    print("\n".join(lines[:20]))


if __name__ == "__main__":
    main()
