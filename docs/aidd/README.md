# AIDD workspace

This folder holds the AI-Driven-Development artifacts for the Aseprite pixel-art plugin.

- [`COMPLETENESS_CHECKLIST.md`](COMPLETENESS_CHECKLIST.md) — living scorecard (re-score every milestone).
- [`PROJECT_PLAN.md`](PROJECT_PLAN.md) — vision, target architecture, AIDD method, phased roadmap.
- [`../adr/`](../adr/) — Architecture Decision Records.
- [`../../specs/`](../../specs/) — spec-first feature specs (source of truth before code).

## Workflow
1. New feature → write a spec in `specs/` (use `specs/TEMPLATE.md`), link the checklist item(s) it advances.
2. Record any structural/irreversible choice as an ADR.
3. Implement → add tests/evals → re-score the checklist → commit the delta.
4. PRs reference: spec ID, checklist item(s), ADR(s).
