# Events and rules

A model can say more than its reactions: an event that changes values when
something happens, a rule that always sets a value, a starting value worked
out from a formula. PathwayDesigner keeps all of these, reads them from SBML
and Antimony and writes them back, so a model opened and saved means the
same.

## The Events & Rules tab

The **Events & Rules** tab in the panel on the right lists them, each as a
sentence:

- **Events**: "When time ≥ 10: Glucose becomes 5", with a delay or priority
  underneath if it has one.
- **Rules**: "y is always A + B", "Volume changes at the rate 0.01".
- **Starting values from formulas**: "ATP starts at 2*Glucose".
- **Functions**, **Units** and **Checks** (conditions the model should keep
  to, with the message to give if it doesn't), when the model has any.

Click a sentence to change it. A small form opens in its place, with a field
for each formula. A mistake, such as a name that isn't in the model, is
shown in red under its field, and **Done** waits until it is put right.
**Done** is one step for **Edit ▸ Undo**; **Delete** takes the part out.
**Add an event** and **Add a rule** start new ones; a name that isn't in the
model yet becomes a new parameter.

## On the diagram

A species or compartment that a rule or an event also changes has a small
lightning bolt beside it. **View ▸ Rule and Event Marks** hides them. Its
properties say what else changes it ("Also changed by: the event Feed"), and
clicking that opens it in the tab. The **Structure** tab, which works from
the reactions only, names the species they also change.

Deleting a species can take with it a rule or an event that used it, since
the model would make no sense otherwise. The status bar says what went, and
**Edit ▸ Undo** brings it all back.

## Functions

**File ▸ Preferences…** has **Write functions into the formulas that use
them**, for tools that don't read functions. With it ticked, saved SBML and
Antimony have each call written out in full; the diagram keeps its functions.

Back to the [contents](contents.md).
