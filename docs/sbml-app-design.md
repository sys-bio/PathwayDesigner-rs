# Events, rules and the rest in the app — design

2026-11-07. Design; A1, A2, A4 and A5 settled with the author, the rest as proposed.
All five sub-steps built (2026-11-07), each tested, broken on purpose and shown
in the browser. Step 4 of docs/sbml-full-support-design.md:
the model, SBML and Antimony now hold events, rules, functions, units and
the rest (steps 1–3); the app doesn't show them yet, except as text in the
Antimony tab. F9 and F11 there settle the outline: a right-panel tab,
**Events & Rules**, of plain sentences, edited in place; a species'
properties say what else changes it; a mark on the diagram; a note in the
Structure tab.

## What is shown, and how it reads

The tab lists the model's parts under plain headings, each part as one
sentence, in the model's order. A heading with nothing under it isn't shown,
except Events and Rules, which say "None".

| Heading | One sentence each | SBML |
|---|---|---|
| Events | When time > 10: Glucose becomes 5, ATP becomes ATP + 1 | event |
| Rules | y is always A + B | assignment rule |
| | Volume changes at the rate k*Glucose | rate rule |
| | w − 2*y is kept at 0 | algebraic rule |
| Starting values from formulas | B starts at 2*A | initial assignment |
| Functions | mm(s, v) = v*s/(1 + s) | function definition |
| Units | minute = 60 second | unit definition |
| Checks | A ≥ 0, else: "A went negative" | constraint |

An event's other parts follow its sentence in small type, only where they
aren't the default: "after a delay of 2", "first among events at the same
time: priority 1", "values from when it fires" / "from when it was
triggered", "can fire at the start", "stops if its condition stops being
true". Names are shown as the diagram shows them (id or display name, as
View ▸ Labels says); a named event shows its name before the sentence.

Formulas are shown as written, with `>=`, `&&` and so on as ≥, and, or…
(display only; the text is unchanged).

## Decisions

- **A1. Editing.** *Proposed:* clicking a sentence opens it in place as a
  small form: one text field per formula (the event's condition, each
  assignment, delay, priority; a rule's variable and formula), ticks for the
  event's flags, and Done, Cancel and Delete. Each field is checked as typed
  (unknown names and errors in red, as in the Antimony tab). Done is one undo
  step. Under each heading, **Add …** ("Add an event", "Add a rule") opens an
  empty form. *Settled.*
- **A2. The diagram mark.** A species or compartment set by a rule, or
  changed by an event, is marked. *Settled:* a small lightning bolt at
  the node's top-right corner, outside the node, in the
  reaction-line colour, with hover text "Also changed by …". Hidden with
  View ▸ Rule and Event Marks (on by default). Not in exports (PNG, PDF,
  SBML), like annotation marks.
- **A3. "Also changed by".** A species', compartment's or parameter's
  properties get one line, "Also changed by: the event Feed, its rate rule",
  each a link that opens the Events & Rules tab at it. Its starting value,
  if a formula gives it, is shown as "starts at 2*A (now 2)", editable there
  as now.
- **A4. Writing functions into formulas** (F1). *Proposed:* Preferences ▸
  "Write functions into the formulas that use them", off by default, applies
  when saving SBML and Antimony; the model keeps its functions. *Settled.*
- **A5. The catalytic rewrite** (F6). The preference "Convert catalytic
  reactions" goes. *Settled:* no command either. `A -> A + B` and `-> B`
  with A as a modifier mean the same, but they are different reactions, and
  the app doesn't change the reactions a file or text states; drawing A as
  a catalyst is the diagram's job (self-modifier loops, Place Modifiers by
  Their Reactions). The import option and the re-import when the preference
  changed go with it.
- **A6. The Structure tab** (F11). Under the analysis: "Also changed by
  rules or events, which the analysis leaves out: Glucose, ATP."
- **A7. Deleting** (F8). What went with a deleted item is said in the status
  bar: "Also deleted, as they used S1: the rule for y, the event Feed." One
  undo brings it all back.
- **A8. Opening warnings.** Only what is still true stays: what Level 2
  says that Level 3 can't (reported as now), a model built from submodels
  (`comp`), which is kept but can't be edited as one. The warnings about
  events, rules and functions dropped go.

## Sub-steps

1. The tab, showing (no editing); A6.
2. Editing in place (A1).
3. A3 and the diagram mark (A2).
4. A7, A8.
5. A4, A5 (the preference and the import option removed).

Each tested, broken on purpose, and shown in the browser.
