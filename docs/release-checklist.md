# Release checklist (S §17.4)

The conformance checklist of SPECIFICATION.md §17.4, with the evidence for each
item. "Tests" are run by CI on every push (`cargo test --workspace` unless named
otherwise). The browser build is checked in headless Chromium by
`crates/pd-app/tests/web_smoke.py` (CI job `web`). The drawing in the browser
comes from the same `pd-core` code as the stored reference images
(`crates/pd-export/tests/snapshots`).

Last full pass: R12, 2026-10-10.

| # | Item | Evidence | By hand |
|---|---|---|---|
| 1 | Every `Models/*.json` loads and saves back unchanged | `pd-core/tests/models_round_trip.rs` | — |
| 2 | Every `Models/*.ant` imports; export and re-import keep the biochemistry | `antimony::tests::every_reference_file_imports_and_round_trips` | — |
| 3 | Render modes: Straight, Linear (sliding junction, hidden handle), Bézier (clipped, tangent arrowheads) | `draw` tests (`linear_reaction_is_one_line_with_no_junction`, `arrowhead_lies_along_the_curve_at_the_border`, `cut_reactant_leg_starts_outside_its_node`), `effective_junction_of_a_linear_reaction`, `a_linear_junction_keeps_its_place_along_the_line`; reference images of `model6` (Linear) and `model12` (Bézier) | Look at a sample in the browser |
| 4 | Aliases: purple and dashed with the indicator on, primary's label, deleting a primary deletes its aliases, one SBML glyph per alias | `aliases_show_the_primary_id_with_a_dashed_border`, `deleting_a_primary_removes_its_aliases_and_their_reactions`, `create_alias_and_go_to_primary`; SBML round trip keeps alias ids (`sbml_round_trip.rs`); reference image of `testmodel` | — |
| 5 | Null nodes: created on import, hidden by the preference, left out of Antimony export | `null_nodes_for_empty_sides`, `null_nodes_fill_empty_sides`, `null_nodes_and_their_legs_can_be_hidden`, `hidden_null_nodes_and_their_legs_cant_be_hit`; Antimony export test | Toggle *Show null species* in Preferences |
| 6 | Modifiers: detected on import, synced when the rate law is edited, drawn as dashed arcs ending in a dot | `implied_parameters_and_modifiers`, `law_species_become_modifiers_once`, `modifier_arc_is_dashed_and_ends_at_a_dot`, modifier-sync tests in `view`; reference image of `modifer2` | Edit a rate law in the inspector |
| 7 | Every model change can be undone and redone; Open and New clear the history | `undo` tests, `opening_a_model_clears_the_history`, one-undo-step tests for import, layout, nudge, delete, align | — |
| 8 | Hit-test priority and selection rules (S §6.2–6.3) | `view::interaction_tests` (`hit_order`, click and rubber-band rules) | — |
| 9 | Align and distribute with junction and handle updates | `view::arrange` tests, including junction alignment and `partial_moves_reset_bezier_handles` | — |
| 10 | Auto-layout: deduplicated springs, locked nodes stay put, no drift, Deckard pass | `layout` tests (`running_twice_does_not_drift`, `locked_nodes_do_not_move`, `deckard_*`), `auto_layout_animates_and_is_one_undo_step` | — |
| 11 | SBML passes validation; layout curves start and end on node borders | CI job `sbml`: every model exported and checked with python-libsbml `checkConsistency()`, no errors; curves are written from `draw::border` (`sbml/export.rs`) | Open an export in COPASI or SBMLLayout |
| 12 | PNG at the chosen DPI; vector PDF fitted to the page | `png_size_follows_the_dpi`, `render::pdf` tests (`pages_and_fitting`); CI job `graphics`: pypdfium2 draws every PDF like resvg's PNG | Print from the browser |
| 13 | Locale-independent number formatting in every output | Every writer formats numbers with Rust's `format!`, which never uses the locale (JSON through serde, SBML through `sbml_io::fmt_f64`, PDF through `render::pdf::numf`, Antimony through `antimony::number`) | — |
| 14 | Preferences saved, reloaded and applied at startup | `prefs::tests::saved_values_come_back`; `PathwayApp::new` applies them (fixes D-2) | Change a preference, reload the page |

## Web release (SPEC-Rust §14.2)

| Item | Evidence |
|---|---|
| Served from a sub-path | `web_smoke.py` serves the build at `/PathwayDesigner/`; CI deploys with the repository's name |
| Starts without errors | `web_smoke.py` |
| Works offline | `web_smoke.py`: the service worker takes over, and the app starts with the network off |
| "New version available" | `web_smoke.py`: the banner appears when the published page names another build |
| Third-party licences | `THIRD-PARTY.html` ships with the app and is linked from the About box; CI checks it matches the dependencies (`scripts/third_party.py --check`) |
