---
description: "Diagnose failures, identify root cause, apply a minimal fix, and verify."
name: "Debug"
agent: "Engineer"
---
Investigate the reported failure and resolve it with the smallest correct change.

Workflow:
1. Reproduce or inspect the failure signal.
2. Isolate root cause in the narrowest affected scope.
3. Implement a focused fix that preserves existing behavior.
4. Run targeted verification, then broader checks as needed.

Semantic Versioning Directive:
- Default to PATCH for defect fixes.
- Use MINOR only if a backward-compatible capability is added as part of the fix.
- Use MAJOR only when a breaking change is unavoidable and explicitly approved.
