"""Checks PathwayDesigner's structural analysis (crates/pd-core/src/structure.rs)
against libRoadRunner, model by model (docs/structural-analysis-design.md §7).

For each model, the `structure_report` example gives our stoichiometry matrix,
rank, independent and dependent species and reactions, L0 and K0, and writes
the model as SBML; libRoadRunner loads that SBML and gives its own. Compared:

- N, rows and columns matched by id (same species, same reactions, same
  entries);
- the rank;
- the independent species, as sets and in order, and L0 entry by entry when
  the same species are independent (reported only: ties among equal-norm
  columns are broken differently, so another valid choice is not a failure);
- the flux null space: our basis and libRoadRunner's K matrix span the same
  space (and N v = 0 for ours);
- the conservation laws: ours and libRoadRunner's conservation matrix span the
  same space.

    python scripts/structure_check.py <model> [<model> ...]

Needs libRoadRunner (`pip install libroadrunner`) and a release build of the
example (`cargo build -p pd-core --release --example structure_report`).
"""
import json
import os
import subprocess
import sys
import tempfile

import numpy as np
import roadrunner

HERE = os.path.dirname(os.path.abspath(__file__))
EXE = os.path.join(HERE, "..", "target", "release", "examples", "structure_report")
TOL = 1e-8


def ours(path, xml):
    out = subprocess.run([EXE, path, xml], capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


def rank(m):
    return 0 if m.size == 0 else int(np.linalg.matrix_rank(m, tol=1e-8))


def same_span(a, b):
    """Rows of a and rows of b span the same space."""
    if a.shape[0] == 0 or b.shape[0] == 0:
        return a.shape[0] == b.shape[0] == 0 or (rank(a) == 0 and rank(b) == 0)
    ra, rb = rank(a), rank(b)
    return ra == rb == rank(np.vstack([a, b]))


def check(path):
    tmp = tempfile.mkdtemp()
    xml = os.path.join(tmp, "model.xml")
    o = ours(path, xml)
    # An SBML model is compared with libRoadRunner's reading of the original
    # file, so the check covers PathwayDesigner's SBML import too; anything
    # else with libRoadRunner's reading of PathwayDesigner's export.
    original = path.lower().endswith((".xml", ".sbml"))
    r = roadrunner.RoadRunner(path if original else xml)
    row = {"model": os.path.basename(path), "species": len(o["species"]),
           "reactions": len(o["reactions"]), "rank": o["rank"],
           "seconds": o["seconds"], "sparse_seconds": o["sparse_seconds"]}

    # N, matched by id. A model with no floating species has none in
    # libRoadRunner either (it refuses to build the matrix).
    try:
        nr = r.getFullStoichiometryMatrix()
    except RuntimeError as e:
        if "does not exist" in str(e) and not o["species"]:
            row.update({k: True for k in ("same species", "same reactions", "N equal",
                                          "rank equal", "N v = 0",
                                          "null space same span", "laws same span")})
            row["note"] = "no floating species in either"
            return row
        raise
    sp, rx = list(nr.rownames), list(nr.colnames)
    # Species declared constant are not variables, so PathwayDesigner leaves
    # them out of N; libRoadRunner keeps them as rows of zeros (they can only
    # be modifiers). Those rows are dropped here, and counted.
    n_rr_all = np.array(nr) if nr.shape[0] else np.zeros((0, len(rx)))
    extra = [i for i, s in enumerate(sp) if s not in o["species"]]
    zero_extra = [i for i in extra if not np.any(n_rr_all[i, :])]
    if zero_extra and len(zero_extra) == len(extra):
        keep = [i for i in range(len(sp)) if i not in zero_extra]
        dropped = [sp[i] for i in zero_extra]
        row["note"] = f"{len(dropped)} constant species (rows of zeros) left out: {dropped}"
        sp = [sp[i] for i in keep]
        nr_kept = n_rr_all[keep, :]
    else:
        dropped = []
        nr_kept = n_rr_all
    row["same species"] = sorted(sp) == sorted(o["species"])
    row["same reactions"] = sorted(rx) == sorted(o["reactions"])
    n_ours = np.array(o["n"], dtype=float).reshape(len(o["species"]), len(o["reactions"]))
    if row["same species"] and row["same reactions"]:
        ri = [o["species"].index(s) for s in sp]
        ci = [o["reactions"].index(c) for c in rx]
        n_matched = n_ours[np.ix_(ri, ci)] if n_ours.size else np.zeros((len(sp), len(rx)))
        row["N equal"] = bool(np.allclose(n_matched, nr_kept, atol=TOL))
    else:
        row["N equal"] = False
        row["note"] = (f"species only ours {sorted(set(o['species']) - set(sp))}, "
                       f"only rr {sorted(set(sp) - set(o['species']))}; "
                       f"reactions only ours {sorted(set(o['reactions']) - set(rx))}, "
                       f"only rr {sorted(set(rx) - set(o['reactions']))}")
        return row

    # libRoadRunner's moiety analysis, where it will do it: it refuses models
    # with species set by rules or variable stoichiometry; then the rank and
    # the conservation laws are checked with NumPy instead (SVD).
    try:
        r.conservedMoietyAnalysis = True
        moieties = True
    except RuntimeError as e:
        moieties = False
        row["note"] = (row.get("note", "") + "; " if "note" in row else "") +             "libRoadRunner moiety analysis refused (" + str(e).split(".")[0][:80] +             "), rank and laws checked with NumPy"
    if moieties:
        indep = [s for s in r.getIndependentFloatingSpeciesIds() if s not in dropped]
    else:
        indep = None
    if indep is None:
        row["rank equal"] = rank(nr_kept) == o["rank"]
        row["same independent species"] = row["same independent set"] = None
    else:
        row["rank equal"] = len(indep) == o["rank"]
        row["same independent species"] = indep == o["independent_species"]
        row["same independent set"] = sorted(indep) == sorted(o["independent_species"])
    if row["same independent species"] and o["dependent_species"] and not dropped:
        l0r = r.getL0Matrix()
        lr = np.array(l0r)
        rows = [o["dependent_species"].index(s) for s in l0r.rownames]
        cols = [o["independent_species"].index(s) for s in l0r.colnames]
        lo = np.array(o["l0"], dtype=float)[np.ix_(rows, cols)]
        row["L0 equal"] = bool(np.allclose(lo, lr, atol=TOL))
    elif row["same independent species"]:
        row["L0 equal"] = True

    # The flux null space: ours as full vectors over rx.
    dep, ind = o["dependent_reactions"], o["independent_reactions"]
    k0 = np.array(o["k0"], dtype=float).reshape(len(dep), len(ind))
    basis = np.zeros((len(ind), len(rx)))
    for j, name in enumerate(ind):
        basis[j, rx.index(name)] = 1.0
        for i, d in enumerate(dep):
            basis[j, rx.index(d)] = k0[i, j]
    n_rr = nr_kept
    row["N v = 0"] = bool(basis.size == 0 or np.allclose(n_rr @ basis.T, 0.0, atol=1e-7))
    # The sparse basis: the same space, and N v = 0.
    sparse = np.array(o["sparse_null_space"], dtype=float).reshape(-1, len(o["reactions"]))
    if sparse.size:
        sparse = sparse[:, [o["reactions"].index(c) for c in rx]]
    row["sparse N v = 0"] = bool(sparse.size == 0 or np.allclose(n_rr @ sparse.T, 0.0, atol=1e-7))
    row["sparse same span"] = same_span(basis, sparse) if basis.size else sparse.size == 0
    ks = o["k_support"]
    ss = [int(np.count_nonzero(v)) for v in sparse]
    row["support"] = (sum(ks) / len(ks), sum(ss) / len(ss)) if ks else (0.0, 0.0)
    kr = r.getKMatrix()
    k_rr = np.zeros((np.array(kr).shape[1], len(rx)))
    for i, name in enumerate(kr.rownames):
        k_rr[:, rx.index(name)] = np.array(kr)[i, :]
    row["null space same span"] = same_span(basis, k_rr)

    # Conservation laws: ours as full vectors over sp.
    laws = np.zeros((len(o["dependent_species"]), len(sp)))
    l0 = np.array(o["l0"], dtype=float).reshape(len(o["dependent_species"]),
                                                len(o["independent_species"]))
    for d, name in enumerate(o["dependent_species"]):
        laws[d, sp.index(name)] = 1.0
        for i, s in enumerate(o["independent_species"]):
            laws[d, sp.index(s)] -= l0[d, i]
    if not moieties:
        # The left null space of N, from the SVD.
        if nr_kept.size:
            u, sv, _ = np.linalg.svd(nr_kept)
            k = int(np.sum(sv > 1e-8))
            g_np = u[:, k:].T
        else:
            g_np = np.eye(len(sp))
        row["laws same span"] = same_span(laws, g_np) if laws.shape[0] or g_np.shape[0] else True
    elif laws.shape[0]:
        g = r.getConservationMatrix()
        g_rr = np.zeros((np.array(g).shape[0], len(sp)))
        for j, name in enumerate(g.colnames):
            if name in sp:
                g_rr[:, sp.index(name)] = np.array(g)[:, j]
        # The dropped constants' own laws ("X = constant") are rows of zeros
        # now.
        g_rr = g_rr[np.any(np.abs(g_rr) > TOL, axis=1)]
        row["laws same span"] = same_span(laws, g_rr)
    else:
        deps = [s for s in r.getDependentFloatingSpeciesIds() if s not in dropped]
        row["laws same span"] = len(deps) == 0
    # The sparse laws span the same space as ours.
    sl = np.array(o["sparse_laws"], dtype=float).reshape(-1, len(o["species"]))
    if sl.size:
        sl = sl[:, [o["species"].index(x) for x in sp]]
    row["sparse laws same span"] = same_span(laws, sl) if laws.size else sl.size == 0
    return row


def o_sparse_ms(row):
    return row.get("sparse_seconds", 0.0) * 1000


def main():
    # What must agree. Which species are independent need not: among
    # columns of equal norm the pivoting's choice is a tie, which LAPACK
    # breaks by its rounding and we break by the model's order, so the bases
    # can differ while spanning the same spaces; that is reported, not failed.
    keys = ["same species", "same reactions", "N equal", "rank equal",
            "N v = 0", "null space same span", "laws same span",
            "sparse N v = 0", "sparse same span", "sparse laws same span"]
    allok = True
    for path in sys.argv[1:]:
        try:
            row = check(path)
        except Exception as e:  # noqa: BLE001 - report and go on
            print(f"{os.path.basename(path)}: ERROR {type(e).__name__}: {str(e)[:200]}")
            allok = False
            continue
        bad = [k for k in keys if row.get(k) is False]
        allok &= not bad
        choice = ("independent species not compared" if row.get("same independent species") is None
                  else "same independent species" if row.get("same independent species")
                  else "same independent set" if row.get("same independent set")
                  else "other independent species (a tie)")
        if row.get("L0 equal") is False:
            choice += ", L0 differs"
        print(f"{row['model']}: {row['species']} species, {row['reactions']} reactions, "
              f"rank {row['rank']}, {row['seconds'] * 1000:.2f} ms: "
              + ("agree" if not bad else "DIFFER: " + ", ".join(bad))
              + f"; {choice}"
              + (f"; reactions per vector {row['support'][0]:.1f} -> {row['support'][1]:.1f}"
                 f" (sparse {o_sparse_ms(row):.1f} ms)" if "support" in row else "")
              + (f" ({row['note']})" if "note" in row else ""))
    sys.exit(0 if allok else 1)


if __name__ == "__main__":
    main()
