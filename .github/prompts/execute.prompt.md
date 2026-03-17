---
description: "Execute the next unchecked implementation step with verification and checklist updates."
name: "Execute"
agent: "Engineer"
---
Read `IMPLEMENTATION.md`, find the first incomplete phase, and execute the first unchecked step.

Execution Rules:
- Gather context in target files before edits.
- Implement complete code for only the selected step unless explicitly instructed otherwise.
- Run required verification commands for the selected step/phase.
- If verification fails, fix relevant issues and rerun.
- Update `IMPLEMENTATION.md` checkbox only after successful validation.

Semantic Versioning Directive:
- Do not bump versions during intermediate phase work.
- Bump only when final project-completion criteria explicitly require release finalization.
- Apply SemVer: PATCH for fixes, MINOR for additive non-breaking features, MAJOR for breaking changes.
