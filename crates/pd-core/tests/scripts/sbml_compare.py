"""Compares SBML files with PathwayDesigner's round trip of them, element by
element, with libSBML (docs/sbml-full-support-design.md F14).

    cargo run -p pd-core --release --example biomodels_roundtrip -- <originals> <out>
    python crates/pd-core/tests/scripts/sbml_compare.py <originals> <out> <report.md>

`<originals>` is a folder of `<id>.xml`, or of `<id>/<id>_url.xml` as
BioModels' curated archive unpacks; `<out>` holds the round trip's `<id>.xml`
and its `roundtrip.jsonl`.

The original is first converted by libSBML to the round trip's level and
version, so that what Level 2 leaves to defaults is compared as Level 3 says
it. Then every element of SBML core is compared: the model and its units,
function and unit definitions, compartments, species, parameters, initial
assignments, rules, constraints, reactions with their species references,
rate laws and local parameters, and events; on each, its name, SBO term,
notes, database references, history, other tools' annotations and metaid.
Layout and render are left out: PathwayDesigner writes those from its own
drawing.

Formulas are compared by value: each is worked out at three sets of random
values for the names it uses, so the same formula written in another form
(`root(3, x)` as `x^(1/3)`) is the same. A formula that can't be worked out
(`delay`, `rateOf`) is compared as libSBML writes it.

An extra `metaid` in the round trip is not a difference. Writes the report
as Markdown, every file's differences as JSON next to it, and exits with 1 if
any file differs.
"""

import json
import math
import os
import random
import re
import sys

import libsbml


# ----- Formulas ---------------------------------------------------------------

class Unknown(Exception):
    pass


def evaluate(node, env, functions, depth=0):
    """The value of a libSBML AST, names from `env`."""
    if depth > 60:
        raise Unknown("too deep")
    t = node.getType()
    kids = [node.getChild(i) for i in range(node.getNumChildren())]
    ev = lambda n: evaluate(n, env, functions, depth + 1)
    if t in (libsbml.AST_INTEGER, libsbml.AST_REAL, libsbml.AST_REAL_E, libsbml.AST_RATIONAL):
        return node.getValue()
    if t == libsbml.AST_NAME_TIME:
        return env.setdefault("\0time", random.uniform(0.5, 2.0))
    if t == libsbml.AST_NAME_AVOGADRO:
        return 6.02214179e23
    if t == libsbml.AST_NAME:
        return env.setdefault(node.getName(), random.uniform(0.5, 2.0))
    if t == libsbml.AST_CONSTANT_PI:
        return math.pi
    if t == libsbml.AST_CONSTANT_E:
        return math.e
    if t == libsbml.AST_CONSTANT_TRUE:
        return 1.0
    if t == libsbml.AST_CONSTANT_FALSE:
        return 0.0
    if t == libsbml.AST_PLUS:
        return sum(ev(k) for k in kids)
    if t == libsbml.AST_TIMES:
        v = 1.0
        for k in kids:
            v *= ev(k)
        return v
    if t == libsbml.AST_MINUS:
        return -ev(kids[0]) if len(kids) == 1 else ev(kids[0]) - ev(kids[1])
    if t == libsbml.AST_DIVIDE:
        return ev(kids[0]) / ev(kids[1])
    if t in (libsbml.AST_POWER, libsbml.AST_FUNCTION_POWER):
        return ev(kids[0]) ** ev(kids[1])
    if t == libsbml.AST_FUNCTION_ROOT:
        if len(kids) == 1:
            return math.sqrt(ev(kids[0]))
        return ev(kids[1]) ** (1.0 / ev(kids[0]))
    if t == libsbml.AST_FUNCTION_LOG:
        if len(kids) == 1:
            return math.log10(ev(kids[0]))
        return math.log(ev(kids[1])) / math.log(ev(kids[0]))
    one = {
        libsbml.AST_FUNCTION_EXP: math.exp, libsbml.AST_FUNCTION_LN: math.log,
        libsbml.AST_FUNCTION_ABS: abs, libsbml.AST_FUNCTION_FLOOR: math.floor,
        libsbml.AST_FUNCTION_CEILING: math.ceil, libsbml.AST_FUNCTION_SIN: math.sin,
        libsbml.AST_FUNCTION_COS: math.cos, libsbml.AST_FUNCTION_TAN: math.tan,
        libsbml.AST_FUNCTION_ARCSIN: math.asin, libsbml.AST_FUNCTION_ARCCOS: math.acos,
        libsbml.AST_FUNCTION_ARCTAN: math.atan, libsbml.AST_FUNCTION_SINH: math.sinh,
        libsbml.AST_FUNCTION_COSH: math.cosh, libsbml.AST_FUNCTION_TANH: math.tanh,
        libsbml.AST_FUNCTION_FACTORIAL: lambda x: math.gamma(x + 1),
    }
    if t in one:
        return one[t](ev(kids[0]))
    if t == libsbml.AST_FUNCTION_MAX:
        return max(ev(k) for k in kids)
    if t == libsbml.AST_FUNCTION_MIN:
        return min(ev(k) for k in kids)
    rel = {
        libsbml.AST_RELATIONAL_EQ: lambda a, b: a == b, libsbml.AST_RELATIONAL_NEQ: lambda a, b: a != b,
        libsbml.AST_RELATIONAL_LT: lambda a, b: a < b, libsbml.AST_RELATIONAL_LEQ: lambda a, b: a <= b,
        libsbml.AST_RELATIONAL_GT: lambda a, b: a > b, libsbml.AST_RELATIONAL_GEQ: lambda a, b: a >= b,
    }
    if t in rel:
        vals = [ev(k) for k in kids]
        return float(all(rel[t](a, b) for a, b in zip(vals, vals[1:])))
    if t == libsbml.AST_LOGICAL_AND:
        return float(all(ev(k) != 0 for k in kids))
    if t == libsbml.AST_LOGICAL_OR:
        return float(any(ev(k) != 0 for k in kids))
    if t == libsbml.AST_LOGICAL_XOR:
        return float(sum(ev(k) != 0 for k in kids) % 2 == 1)
    if t == libsbml.AST_LOGICAL_NOT:
        return float(ev(kids[0]) == 0)
    if t == libsbml.AST_FUNCTION_PIECEWISE:
        n = len(kids)
        for i in range(0, n - 1, 2):
            if ev(kids[i + 1]) != 0:
                return ev(kids[i])
        if n % 2 == 1:
            return ev(kids[-1])
        raise Unknown("no piece")
    if t == libsbml.AST_FUNCTION:
        f = functions.get(node.getName())
        if f is None:
            raise Unknown("function " + node.getName())
        args, body = f
        inner = dict(env)
        for a, k in zip(args, kids):
            inner[a] = ev(k)
        return evaluate(body, inner, functions, depth + 1)
    raise Unknown(f"node type {t}")


def functions_of(model):
    out = {}
    for f in model.getListOfFunctionDefinitions():
        lam = f.getMath()
        if lam is None or lam.getNumChildren() == 0:
            continue
        n = lam.getNumChildren()
        out[f.getId()] = ([lam.getChild(i).getName() for i in range(n - 1)], lam.getChild(n - 1))
    return out


def same_math(a, b, fa, fb):
    if a is None or b is None:
        return (a is None) == (b is None)
    sa, sb = libsbml.formulaToL3String(a), libsbml.formulaToL3String(b)
    if sa == sb:
        return True
    try:
        for seed in (1, 2, 3):
            random.seed(seed)
            env = {}
            va = evaluate(a, env, fa)
            vb = evaluate(b, env, fb)
            if isinstance(va, float) and isinstance(vb, float) and math.isnan(va) and math.isnan(vb):
                continue
            if va != vb and not (abs(va - vb) <= 1e-9 * max(abs(va), abs(vb), 1e-300)):
                return False
        return True
    except (Unknown, ValueError, ZeroDivisionError, OverflowError, TypeError):
        return False


def formula(ast):
    return libsbml.formulaToL3String(ast) if ast is not None else None


# ----- What any element carries ------------------------------------------------

def squeeze(xml):
    return re.sub(r">\s+<", "><", xml or "").strip()


def cv_terms(e):
    """Each (qualifier, resource) said, however the RDF groups them."""
    out = set()
    for i in range(e.getNumCVTerms()):
        t = e.getCVTerm(i)
        q = t.getBiologicalQualifierType() if t.getQualifierType() == libsbml.BIOLOGICAL_QUALIFIER else t.getModelQualifierType()
        for j in range(t.getNumResources()):
            out.add((t.getQualifierType(), q, t.getResourceURI(j)))
    return sorted(out)


def history(e):
    if not e.isSetModelHistory():
        return None
    h = e.getModelHistory()
    # Names compared without spaces at their ends, which the RDF reader
    # trims ("Lauren ").
    creators = sorted(
        (c.getFamilyName().strip(), c.getGivenName().strip(), c.getEmail().strip(), c.getOrganisation().strip())
        for c in (h.getCreator(i) for i in range(h.getNumCreators()))
    )
    created = h.getCreatedDate().getDateAsString() if h.isSetCreatedDate() else None
    modified = sorted(h.getModifiedDate(i).getDateAsString() for i in range(h.getNumModifiedDates()))
    return (creators, created, modified)


# Annotations PathwayDesigner drops on purpose (docs/other-annotations-design.md
# O1): CellDesigner's and JDesigner's own drawings, which its edits would make
# wrong.
DROPPED = ("http://www.sbml.org/2001/ns/celldesigner", "http://www.sys-bio.org/sbml",
           # A Level 2 file's layout and render, in its model's annotation:
           # PathwayDesigner writes these from its own drawing.
           "http://projects.eml.org/bcb/sbml/level2",
           "http://projects.eml.org/bcb/sbml/render/level2")


def others(e):
    """The annotation's elements other than RDF, each as text: the first in
    each namespace, as Level 3 allows only one (O1)."""
    a = e.getAnnotation()
    if a is None:
        return []
    out, seen = [], set()
    for i in range(a.getNumChildren()):
        c = a.getChild(i)
        uri = c.getURI()
        # Text straight in an annotation isn't SBML; only elements count.
        if not c.isElement() or c.getName() == "RDF" or uri in DROPPED or uri in seen:
            continue
        seen.add(uri)
        out.append(canonical(c))
    return sorted(out)


def canonical(node):
    """An XML element as text with its names resolved to their namespaces,
    so the same element is the same text whichever element declares a
    prefix it uses."""
    if node.isText():
        return squeeze(node.getCharacters())
    attrs = sorted(
        (node.getAttrURI(i), node.getAttrName(i), node.getAttrValue(i))
        for i in range(node.getAttributesLength())
    )
    kids = "".join(canonical(node.getChild(i)) for i in range(node.getNumChildren()))
    return f"<{{{node.getURI()}}}{node.getName()} {attrs}>{kids}</>"


def sbase(e, where, diff, b):
    if e.getSBOTerm() != b.getSBOTerm():
        diff(where, "sboTerm", e.getSBOTermID(), b.getSBOTermID())
    if squeeze(e.getNotesString()) != squeeze(b.getNotesString()):
        diff(where, "notes", e.getNotesString()[:60], b.getNotesString()[:60])
    if cv_terms(e) != cv_terms(b):
        diff(where, "references", cv_terms(e), cv_terms(b))
    if history(e) != history(b):
        diff(where, "history", history(e), history(b))
    if others(e) != others(b):
        diff(where, "other annotation", others(e), others(b))
    if e.isSetMetaId() and e.getMetaId() != b.getMetaId():
        diff(where, "metaid", e.getMetaId(), b.getMetaId())


def attributes(e, b, where, diff, names):
    """Each attribute as set or not, and its value."""
    for name in names:
        cap = name[0].upper() + name[1:]
        is_set = getattr(e, "isSet" + cap, None)
        get = getattr(e, "get" + cap)
        ka = (is_set() if is_set else True, get() if (not is_set or is_set()) else None)
        kb = (getattr(b, "isSet" + cap)() if is_set else True,
              getattr(b, "get" + cap)() if (not is_set or getattr(b, "isSet" + cap)()) else None)
        if ka != kb and not (isinstance(ka[1], float) and isinstance(kb[1], float)
                             and ka[0] == kb[0] and close(ka[1], kb[1])):
            diff(where, name, ka[1] if ka[0] else "unset", kb[1] if kb[0] else "unset")


def close(x, y):
    if math.isnan(x) and math.isnan(y):
        return True
    return x == y or abs(x - y) <= 1e-12 * max(abs(x), abs(y))


# ----- The model ----------------------------------------------------------------

def by_id(items):
    return {i.getId(): i for i in items}


def compare_models(a, b):
    diffs = []

    def diff(where, what, x, y):
        diffs.append({"where": where, "what": what, "original": str(x)[:120], "round trip": str(y)[:120]})

    fa, fb = functions_of(a), functions_of(b)

    def math_of(where, what, x, y):
        if not same_math(x, y, fa, fb):
            diff(where, what, formula(x), formula(y))

    attributes(a, b, "model", diff, ["id", "name", "substanceUnits", "timeUnits", "volumeUnits",
                                     "areaUnits", "lengthUnits", "extentUnits", "conversionFactor"])
    sbase(a, "model", diff, b)

    def pair(kind, xs, ys, key, each):
        xs, ys = list(xs), list(ys)
        kx = {key(x): x for x in xs}
        ky = {key(y): y for y in ys}
        for k in kx:
            if k not in ky:
                diff(f"{kind} {k}", "missing", "present", "absent")
        for k in ky:
            if k not in kx:
                diff(f"{kind} {k}", "added", "absent", "present")
        for k in kx:
            if k in ky:
                each(f"{kind} {k}", kx[k], ky[k])

    def function(where, x, y):
        attributes(x, y, where, diff, ["name"])
        sbase(x, where, diff, y)
        lx, ly = x.getMath(), y.getMath()
        if formula(lx) != formula(ly) and not same_lambda(lx, ly, fa, fb):
            diff(where, "math", formula(lx), formula(ly))

    # A function using the time, or a name that isn't its argument (Level 2
    # Version 1 allowed it), can't be one in Level 3: PathwayDesigner writes it
    # into the formulas that call it, which are compared by value as ever.
    def level_3_function(f):
        lam = f.getMath()
        if lam is None or lam.getNumChildren() == 0:
            return True
        n = lam.getNumChildren()
        args = {lam.getChild(i).getName() for i in range(n - 1)}
        def ok(node):
            if node.getType() == libsbml.AST_NAME_TIME:
                return False
            if node.getType() == libsbml.AST_NAME and node.getName() not in args:
                return False
            return all(ok(node.getChild(i)) for i in range(node.getNumChildren()))
        return ok(lam.getChild(n - 1))
    pair("function", [f for f in a.getListOfFunctionDefinitions() if level_3_function(f)],
         b.getListOfFunctionDefinitions(), lambda f: f.getId(), function)

    def unit_definition(where, x, y):
        attributes(x, y, where, diff, ["name"])
        sbase(x, where, diff, y)
        ux = sorted((u.getKind(), u.getExponentAsDouble(), u.getScale(), u.getMultiplier()) for u in x.getListOfUnits())
        uy = sorted((u.getKind(), u.getExponentAsDouble(), u.getScale(), u.getMultiplier()) for u in y.getListOfUnits())
        if ux != uy:
            diff(where, "units", ux, uy)

    # Level 2's built-in units, named by an item but defined nowhere (which
    # Level 3 doesn't allow), are defined as Level 2 has them.
    BUILT_IN = {"time": ("second", 1.0), "length": ("metre", 1.0), "area": ("metre", 2.0),
                "volume": ("litre", 1.0), "substance": ("mole", 1.0)}
    ids_a = {u.getId() for u in a.getListOfUnitDefinitions()}
    def built_in(u):
        if u.getId() in ids_a or u.getId() not in BUILT_IN or u.getNumUnits() != 1:
            return False
        x = u.getUnit(0)
        kind, exponent = BUILT_IN[u.getId()]
        return (libsbml.UnitKind_toString(x.getKind()), x.getExponentAsDouble(), x.getScale(),
                x.getMultiplier()) == (kind, exponent, 0, 1.0)
    pair("unit", a.getListOfUnitDefinitions(),
         [u for u in b.getListOfUnitDefinitions() if not built_in(u)], lambda u: u.getId(), unit_definition)

    def compartment(where, x, y):
        attributes(x, y, where, diff, ["name", "spatialDimensions", "size", "units", "constant"])
        sbase(x, where, diff, y)

    pair("compartment", a.getListOfCompartments(), b.getListOfCompartments(), lambda c: c.getId(), compartment)

    def species(where, x, y):
        attributes(x, y, where, diff, ["name", "compartment", "initialAmount", "initialConcentration",
                                       "substanceUnits", "hasOnlySubstanceUnits", "boundaryCondition",
                                       "constant", "conversionFactor"])
        sbase(x, where, diff, y)

    pair("species", a.getListOfSpecies(), b.getListOfSpecies(), lambda s: s.getId(), species)

    def parameter(where, x, y):
        attributes(x, y, where, diff, ["name", "value", "units", "constant"])
        sbase(x, where, diff, y)

    pair("parameter", a.getListOfParameters(), b.getListOfParameters(), lambda p: p.getId(), parameter)

    def initial(where, x, y):
        sbase(x, where, diff, y)
        math_of(where, "math", x.getMath(), y.getMath())

    pair("initial assignment", a.getListOfInitialAssignments(), b.getListOfInitialAssignments(),
         lambda i: i.getSymbol(), initial)

    def rule_key(r, i=[0]):
        if r.isAlgebraic():
            return f"algebraic {formula(r.getMath())}"
        return ("rate " if r.isRate() else "assignment ") + r.getVariable()

    def rule(where, x, y):
        sbase(x, where, diff, y)
        math_of(where, "math", x.getMath(), y.getMath())

    # Algebraic rules by value, since they have no variable.
    alg_a = [r for r in a.getListOfRules() if r.isAlgebraic()]
    alg_b = [r for r in b.getListOfRules() if r.isAlgebraic()]
    used = set()
    for r in alg_a:
        match = next((i for i, s in enumerate(alg_b) if i not in used and same_math(r.getMath(), s.getMath(), fa, fb)), None)
        if match is None:
            diff(f"algebraic rule {formula(r.getMath())}", "missing", "present", "absent")
        else:
            used.add(match)
            sbase(r, "algebraic rule", diff, alg_b[match])
    for i, s in enumerate(alg_b):
        if i not in used:
            diff(f"algebraic rule {formula(s.getMath())}", "added", "absent", "present")
    pair("rule", [r for r in a.getListOfRules() if not r.isAlgebraic()],
         [r for r in b.getListOfRules() if not r.isAlgebraic()], rule_key, rule)

    def constraint(where, x, y):
        sbase(x, where, diff, y)
        mx = squeeze(x.getMessageString()) if x.isSetMessage() else None
        my = squeeze(y.getMessageString()) if y.isSetMessage() else None
        if mx != my:
            diff(where, "message", mx, my)

    pair("constraint", a.getListOfConstraints(), b.getListOfConstraints(), lambda c: formula(c.getMath()), constraint)

    def reference(r):
        return (r.getSpecies(), r.getId() if r.isSetId() else "")

    def reaction(where, x, y):
        attributes(x, y, where, diff, ["name", "reversible", "compartment"])
        if x.getLevel() == 3 and x.getVersion() == 1 or x.getLevel() < 3:
            if x.isSetFast() and x.getFast() and not (y.isSetFast() and y.getFast()):
                diff(where, "fast", True, y.getFast() if y.isSetFast() else "unset")
        sbase(x, where, diff, y)
        for kind, lx, ly in (("reactant", x.getListOfReactants(), y.getListOfReactants()),
                             ("product", x.getListOfProducts(), y.getListOfProducts())):
            def leg(w, p, q):
                attributes(p, q, w, diff, ["stoichiometry", "constant", "name"])
                if p.isSetId() and p.getId() != q.getId():
                    diff(w, "id", p.getId(), q.getId())
                sbase(p, w, diff, q)
            pair(f"{where} {kind}", lx, ly, lambda r: r.getSpecies(), leg)

        def modifier(w, p, q):
            attributes(p, q, w, diff, ["name"])
            if p.isSetId() and p.getId() != q.getId():
                diff(w, "id", p.getId(), q.getId())
            sbase(p, w, diff, q)
        # A species may be a modifier twice: each by its turn.
        def nth(items):
            seen = {}
            out = []
            for r in items:
                seen[r.getSpecies()] = seen.get(r.getSpecies(), 0) + 1
                out.append((f"{r.getSpecies()}#{seen[r.getSpecies()]}", r))
            return out
        mx, my = dict(nth(x.getListOfModifiers())), dict(nth(y.getListOfModifiers()))
        # One the rate law names but the file didn't list is added (F7).
        law_names = set()
        if x.getKineticLaw() is not None and x.getKineticLaw().getMath() is not None:
            law_names = set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", formula(x.getKineticLaw().getMath())))
        for k in list(my):
            if k not in mx and k.split("#")[0] in law_names:
                del my[k]
        pair(f"{where} modifier", list(mx.items()), list(my.items()), lambda kv: kv[0],
             lambda w, p, q: modifier(w, p[1], q[1]))
        kx, ky = x.getKineticLaw(), y.getKineticLaw()
        if (kx is None) != (ky is None):
            diff(where, "rate law", "present" if kx else "absent", "present" if ky else "absent")
            return
        if kx is None:
            return
        sbase(kx, where + " rate law", diff, ky)
        def local(w, p, q):
            attributes(p, q, w, diff, ["name", "value", "units"])
            sbase(p, w, diff, q)
        pair(f"{where} local parameter", kx.getListOfLocalParameters() if kx.getLevel() == 3 else kx.getListOfParameters(),
             ky.getListOfLocalParameters() if ky.getLevel() == 3 else ky.getListOfParameters(), lambda p: p.getId(), local)
        math_of(where, "rate law", kx.getMath(), ky.getMath())

    pair("reaction", a.getListOfReactions(), b.getListOfReactions(), lambda r: r.getId(), reaction)

    def event_key(e, n={}):
        return e.getId() if e.isSetId() else "trigger " + formula(e.getTrigger().getMath() if e.isSetTrigger() else None)

    def event(where, x, y):
        attributes(x, y, where, diff, ["name", "useValuesFromTriggerTime"])
        sbase(x, where, diff, y)
        tx, ty = x.getTrigger(), y.getTrigger()
        if (tx is None) != (ty is None):
            diff(where, "trigger", tx is not None, ty is not None)
        elif tx is not None:
            attributes(tx, ty, where + " trigger", diff, ["initialValue", "persistent"])
            sbase(tx, where + " trigger", diff, ty)
            math_of(where, "trigger", tx.getMath(), ty.getMath())
        for part in ("Delay", "Priority"):
            px = getattr(x, "get" + part)() if getattr(x, "isSet" + part)() else None
            py = getattr(y, "get" + part)() if getattr(y, "isSet" + part)() else None
            if (px is None) != (py is None):
                diff(where, part.lower(), px is not None, py is not None)
            elif px is not None:
                sbase(px, f"{where} {part.lower()}", diff, py)
                math_of(where, part.lower(), px.getMath(), py.getMath())

        def assignment(w, p, q):
            sbase(p, w, diff, q)
            math_of(w, "math", p.getMath(), q.getMath())
        pair(f"{where} assignment", x.getListOfEventAssignments(), y.getListOfEventAssignments(),
             lambda e: e.getVariable(), assignment)

    # Events without an id, by their turn among them.
    def keyed(events):
        out, n = [], 0
        for e in events:
            if e.isSetId():
                out.append((e.getId(), e))
            else:
                n += 1
                out.append((f"#{n}", e))
        return out
    pair("event", keyed(a.getListOfEvents()), keyed(b.getListOfEvents()), lambda kv: kv[0],
         lambda w, x, y: event(w, x[1], y[1]))
    return diffs


def same_lambda(x, y, fa, fb):
    if x is None or y is None:
        return (x is None) == (y is None)
    n = x.getNumChildren()
    if n != y.getNumChildren():
        return False
    # Arguments may be renamed: compare the bodies with the arguments matched.
    ax = [x.getChild(i).getName() for i in range(n - 1)]
    ay = [y.getChild(i).getName() for i in range(n - 1)]
    try:
        for seed in (1, 2, 3):
            random.seed(seed)
            vals = [random.uniform(0.5, 2.0) for _ in ax]
            va = evaluate(x.getChild(n - 1), dict(zip(ax, vals)), fa)
            vb = evaluate(y.getChild(n - 1), dict(zip(ay, vals)), fb)
            if not close(float(va), float(vb)) and abs(va - vb) > 1e-9 * max(abs(va), abs(vb)):
                return False
        return True
    except (Unknown, ValueError, ZeroDivisionError, OverflowError, TypeError):
        return False


# ----- Other packages (F10) --------------------------------------------------------

def package_parts(path):
    """What other SBML packages (fbc, qual, comp, groups, …) put in a file:
    each attribute and child element in a package's namespace, with the
    core element it is on (its tag, and its id, species or variable).
    Layout and Render, which PathwayDesigner writes from its own drawing,
    and what is inside annotations and notes are left out."""
    import xml.etree.ElementTree as ET

    with open(path, "rb") as fh:
        root = ET.fromstring(fh.read())

    def ns(tag):
        return tag[1:].split("}")[0] if tag.startswith("{") else ""

    def is_core(uri):
        return uri.startswith("http://www.sbml.org/sbml/level") and (
            uri.endswith("/core") or "/level2" in uri or "/level1" in uri)

    def skipped(uri):
        return "/layout/" in uri or "/render/" in uri or uri.startswith("http://projects.eml.org/bcb/sbml")

    def canonical(e):
        attrs = sorted(e.attrib.items())
        kids = "".join(canonical(k) for k in e)
        text = (e.text or "").strip()
        return f"<{e.tag} {attrs}>{text}{kids}</>"

    parts = []

    def walk(e, where):
        local = e.tag.split("}")[-1]
        if local in ("annotation", "notes", "math"):
            return
        key = e.attrib.get("id") or e.attrib.get("species") or e.attrib.get("variable") or ""
        here = f"{where}/{local}[{key}]" if key else f"{where}/{local}"
        for name, value in e.attrib.items():
            uri = ns(name)
            if uri and not is_core(uri) and not skipped(uri):
                parts.append(f"{here} @{name}={value}")
        for k in e:
            uri = ns(k.tag)
            if k.tag.split("}")[-1] in ("annotation", "notes", "math"):
                continue
            if uri and not is_core(uri):
                if not skipped(uri):
                    parts.append(f"{here} {canonical(k)}")
            else:
                walk(k, here)

    walk(root, "")
    return sorted(parts)


def compare_packages(original, roundtrip):
    a, b = package_parts(original), package_parts(roundtrip)
    if a == b:
        return []
    out = []
    for p in sorted(set(a) - set(b))[:5]:
        out.append({"where": "package", "what": "missing", "original": p[:120], "round trip": ""})
    for p in sorted(set(b) - set(a))[:5]:
        out.append({"where": "package", "what": "added", "original": "", "round trip": p[:120]})
    return out


# ----- Files ---------------------------------------------------------------------

def original_file(src, mid):
    """As biomodels_roundtrip finds it: `<id>.xml`, `<id>/<id>_url.xml`, or
    the folder's one SBML file."""
    flat = os.path.join(src, f"{mid}.xml")
    if os.path.exists(flat):
        return flat
    folder = os.path.join(src, mid)
    url = os.path.join(folder, f"{mid}_url.xml")
    if os.path.exists(url):
        return url
    for f in sorted(os.listdir(folder)) if os.path.isdir(folder) else []:
        if not f.endswith(".xml") or f == "manifest.xml" or f.endswith("_urn.xml"):
            continue
        with open(os.path.join(folder, f), "rb") as fh:
            if b"<sbml" in fh.read(2000):
                return os.path.join(folder, f)
    return None


def compare_files(original, roundtrip):
    # Read as text first: libSBML can't open a path with accents on Windows.
    def read(path):
        with open(path, "rb") as fh:
            return libsbml.readSBMLFromString(fh.read().decode("utf-8", "replace"))
    a_doc = read(original)
    b_doc = read(roundtrip)
    if b_doc.getModel() is None:
        return [{"where": "file", "what": "unreadable", "original": "", "round trip": ""}]
    target = (b_doc.getLevel(), b_doc.getVersion())
    if (a_doc.getLevel(), a_doc.getVersion()) != target:
        converted = a_doc.clone()
        if converted.setLevelAndVersion(target[0], target[1], False):
            a_doc = converted
    return compare_models(a_doc.getModel(), b_doc.getModel()) + compare_packages(original, roundtrip)


KINDS = {"package", "model", "function", "unit", "compartment", "species", "parameter", "initial", "assignment",
         "rule", "rate", "algebraic", "constraint", "reaction", "reactant", "product", "modifier",
         "law", "local", "event", "trigger", "delay", "priority", "file", "check"}


def kind_of(where):
    """Where a difference is, ids left out: `reaction R1 reactant S1` is
    `reaction reactant`."""
    return " ".join(w for w in where.split() if w in KINDS) or where


def main():
    src, out, report = sys.argv[1], sys.argv[2], sys.argv[3]
    log = {}
    with open(os.path.join(out, "roundtrip.jsonl"), encoding="utf-8") as fh:
        for line in fh:
            r = json.loads(line)
            log[r["id"]] = r
    results = {}
    for i, mid in enumerate(sorted(log), 1):
        if not log[mid].get("read"):
            results[mid] = [{"where": "file", "what": "not read", "original": "", "round trip": ""}]
            continue
        original = log[mid].get("file") or original_file(src, mid)
        try:
            results[mid] = compare_files(original, os.path.join(out, f"{mid}.xml"))
        except Exception as e:  # a crash in the comparison is a finding too
            results[mid] = [{"where": "check", "what": "crashed", "original": repr(e)[:120], "round trip": ""}]
        if i % 200 == 0:
            print(f"{i} of {len(log)} compared", flush=True)
    with open(report.replace(".md", ".json"), "w", encoding="utf-8") as fh:
        json.dump(results, fh, indent=1)
    same = sorted(m for m, d in results.items() if not d)
    kinds = {}
    for m, ds in results.items():
        for d in ds:
            where = kind_of(d["where"])
            kinds.setdefault((where, d["what"]), set()).add(m)
    lines = [
        "# SBML round trip, element by element",
        "",
        f"{len(results)} files; **{len(same)} with no difference**, {len(results) - len(same)} with some.",
        "",
        "| Element | What differs | Files | For example |",
        "|---|---|---|---|",
    ]
    for (where, what), ms in sorted(kinds.items(), key=lambda kv: -len(kv[1])):
        lines.append(f"| {where} | {what} | {len(ms)} | {', '.join(sorted(ms)[:3])} |")
    with open(report, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"{len(same)} of {len(results)} with no difference")
    sys.exit(0 if len(same) == len(results) else 1)


if __name__ == "__main__":
    main()
