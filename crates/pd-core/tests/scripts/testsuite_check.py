"""The SBML Test Suite through PathwayDesigner: each case's model read and
written back as PathwayDesigner does, simulated with libRoadRunner as the
case's settings say, and compared with the case's published results by the
Test Suite's own rule (|expected - actual| <= absolute + relative*|expected|).

    python crates/pd-core/tests/scripts/testsuite_check.py prepare <suite>/semantic <flat>
    cargo run -p pd-core --release --example biomodels_roundtrip -- <flat> <out>
    python crates/pd-core/tests/scripts/testsuite_check.py check <suite>/semantic <out> <report.md>

`prepare` copies each case's newest SBML file (Level 3 Version 2 down to
Level 2 Version 1) to `<flat>/<case>.xml`. The original is simulated too:
a case libRoadRunner itself fails is no evidence either way. Cases whose
tags show what PathwayDesigner leaves out (events, rate and algebraic rules,
constraints, SBML packages) are reported apart. Each case runs in its own
process, with a time limit.
"""

import concurrent.futures as cf
import json
import os
import shutil
import subprocess
import sys

LEVELS = ["l3v2", "l3v1", "l2v5", "l2v4", "l2v3", "l2v2", "l2v1"]
TIMEOUT = 120
# What PathwayDesigner leaves out, by the cases' component and test tags.
LEFT_OUT = {
    "EventNoDelay": "events",
    "EventWithDelay": "events",
    "RateRule": "rate rules",
    "AlgebraicRule": "algebraic rules",
    "Constraint": "constraints",
    "comp": "the comp package",
    "fbc": "the fbc package",
}


def settings(case_dir, case):
    out = {}
    with open(os.path.join(case_dir, f"{case}-settings.txt"), encoding="utf-8") as fh:
        for line in fh:
            if ":" in line:
                k, v = line.split(":", 1)
                out[k.strip()] = v.strip()
    split = lambda k: [x.strip() for x in out.get(k, "").split(",") if x.strip()]
    return {
        "start": float(out.get("start", 0)),
        "duration": float(out.get("duration", 0)),
        "steps": int(float(out.get("steps", 0))),
        "variables": split("variables"),
        "absolute": float(out.get("absolute", 0) or 0),
        "relative": float(out.get("relative", 0) or 0),
        "amount": set(split("amount")),
    }


def tags(case_dir, case):
    found = []
    with open(os.path.join(case_dir, f"{case}-model.m"), encoding="utf-8") as fh:
        for line in fh:
            if line.startswith(("componentTags:", "testTags:", "packagesPresent:")):
                found += [t.strip() for t in line.split(":", 1)[1].split(",") if t.strip()]
    return found


def expected(case_dir, case):
    with open(os.path.join(case_dir, f"{case}-results.csv"), encoding="utf-8") as fh:
        head = [h.strip() for h in fh.readline().split(",")]
        rows = [[float(x) for x in line.split(",")] for line in fh if line.strip()]
    return head, rows


def simulate(path, s):
    import roadrunner

    rr = roadrunner.RoadRunner(path)
    species = set(rr.model.getFloatingSpeciesIds()) | set(rr.model.getBoundarySpeciesIds())
    sel = ["time"]
    for v in s["variables"]:
        sel.append(v if (v in s["amount"] or v not in species) else f"[{v}]")
    rr.timeCourseSelections = sel
    res = rr.simulate(s["start"], s["start"] + s["duration"], s["steps"] + 1)
    return [list(map(float, row)) for row in res]


def compare(head, want, got, s):
    """The first (variable, time) outside the Test Suite's tolerance, if
    any."""
    for i, row in enumerate(want):
        if i >= len(got):
            return f"only {len(got)} times simulated"
        for j, v in enumerate(s["variables"]):
            e = row[head.index(v)] if v in head else None
            a = got[i][j + 1]
            if e is None:
                continue
            if e != e and a != a:
                continue
            if not abs(e - a) <= s["absolute"] + s["relative"] * abs(e):
                return f"{v} at t={row[0]:g}: expected {e:.6g}, got {a:.6g}"
    return None


def check_one(case_dir, case, original, roundtrip):
    s = settings(case_dir, case)
    head, want = expected(case_dir, case)
    out = {}
    for key, path in (("original", original), ("roundtrip", roundtrip)):
        try:
            out[key] = compare(head, want, simulate(path, s), s) or "pass"
        except Exception as e:  # noqa: BLE001
            out[key] = "error: " + (str(e).splitlines()[0][:160] if str(e) else type(e).__name__)
    return out


def run_one(args):
    case_dir, case, original, roundtrip = args
    try:
        p = subprocess.run(
            [sys.executable, __file__, "one", case_dir, case, original, roundtrip],
            capture_output=True, text=True, timeout=TIMEOUT,
        )
        found = [l for l in p.stdout.splitlines() if l.startswith("RESULT ")]
        r = json.loads(found[-1][7:]) if found else {"original": "crashed", "roundtrip": "crashed"}
    except subprocess.TimeoutExpired:
        r = {"original": "no answer in time", "roundtrip": "no answer in time"}
    r["case"] = case
    return r


def prepare(semantic, flat):
    os.makedirs(flat, exist_ok=True)
    n = 0
    for case in sorted(os.listdir(semantic)):
        d = os.path.join(semantic, case)
        if not os.path.isdir(d):
            continue
        for lv in LEVELS:
            f = os.path.join(d, f"{case}-sbml-{lv}.xml")
            if os.path.exists(f):
                shutil.copy(f, os.path.join(flat, f"{case}.xml"))
                n += 1
                break
    print(f"{n} cases prepared")


def check(semantic, out_dir, report):
    log = {}
    with open(os.path.join(out_dir, "roundtrip.jsonl"), encoding="utf-8") as fh:
        for line in fh:
            r = json.loads(line)
            log[r["id"]] = r
    jobs, results = [], []
    for case, r in sorted(log.items()):
        d = os.path.join(semantic, case)
        original = next(
            (os.path.join(d, f"{case}-sbml-{lv}.xml") for lv in LEVELS if os.path.exists(os.path.join(d, f"{case}-sbml-{lv}.xml"))),
            None,
        )
        if not r["read"]:
            results.append({"case": case, "original": "?", "roundtrip": "not read: " + "; ".join(r.get("errors", []))[:160]})
            continue
        jobs.append((d, case, original, os.path.join(out_dir, f"{case}.xml")))
    with cf.ThreadPoolExecutor(max_workers=max(2, (os.cpu_count() or 4) - 2)) as pool:
        for i, res in enumerate(pool.map(run_one, jobs), 1):
            results.append(res)
            if i % 200 == 0:
                print(f"{i} of {len(jobs)} checked", flush=True)
    said = ("left out", "was read as", "read with the stoichiometry")
    for r in results:
        r["left_out"] = sorted({LEFT_OUT[t] for t in tags(os.path.join(semantic, r["case"]), r["case"]) if t in LEFT_OUT})
        # What the import itself said it left out or changed.
        r["left_out"] += [w for w in log.get(r["case"], {}).get("warnings", []) if any(x in w for x in said)]
    with open(report.replace(".md", ".json"), "w", encoding="utf-8") as fh:
        json.dump(results, fh, indent=1)
    write_report(report, log, results)


def write_report(path, log, results):
    g = {k: [] for k in ("both", "lost", "lost_expected", "gained", "neither", "notread")}
    for r in results:
        o, t = r["original"] == "pass", r["roundtrip"] == "pass"
        if r["roundtrip"].startswith("not read"):
            g["notread"].append(r)
        elif o and t:
            g["both"].append(r)
        elif o and not t:
            g["lost_expected" if r["left_out"] else "lost"].append(r)
        elif t:
            g["gained"].append(r)
        else:
            g["neither"].append(r)
    n = len(results)
    lines = [
        "# The SBML Test Suite through PathwayDesigner and back",
        "",
        f"{n} semantic test cases (SBML Test Suite 3.5.0), each case's newest SBML file read and written back "
        "by PathwayDesigner, then simulated with libRoadRunner and compared with the case's published results "
        "by the Test Suite's tolerances.",
        "",
        "| Result | Cases |",
        "|---|---|",
        f"| Original and round trip both match the published results | {len(g['both'])} |",
        f"| **Original matches, round trip doesn't, with nothing left out** | **{len(g['lost'])}** |",
        f"| Original matches, round trip doesn't: it uses what PathwayDesigner leaves out, as the import says (expected) | {len(g['lost_expected'])} |",
        f"| Original doesn't match in libRoadRunner either | {len(g['neither']) + len(g['gained'])} |",
        f"| Not read by PathwayDesigner | {len(g['notread'])} |",
        f"| Total | {n} |",
        "",
    ]

    def table(title, rows, cols):
        if rows:
            lines.extend([f"## {title}", "", "| Case | " + " | ".join(c[0] for c in cols) + " |", "|---" * (len(cols) + 1) + "|"])
            for r in sorted(rows, key=lambda r: r["case"]):
                lines.append(f"| {r['case']} | " + " | ".join(str(c[1](r)) for c in cols) + " |")
            lines.append("")

    warn = lambda r: "; ".join(log.get(r["case"], {}).get("warnings", []))[:160]
    table("Original matches, round trip doesn't, with nothing left out", g["lost"], [("Round trip", lambda r: r["roundtrip"]), ("Import said", warn)])
    table("Not read by PathwayDesigner", g["notread"], [("Why", lambda r: r["roundtrip"])])
    table("Expected: it uses what PathwayDesigner leaves out", g["lost_expected"], [("Left out", lambda r: ", ".join(r["left_out"]))])
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines))
    print("\n".join(lines[:16]))


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    cmd = sys.argv[1]
    if cmd == "one":
        print("RESULT " + json.dumps(check_one(*sys.argv[2:6])), flush=True)
    elif cmd == "prepare":
        prepare(*sys.argv[2:4])
    else:
        check(*sys.argv[2:5])


if __name__ == "__main__":
    main()
