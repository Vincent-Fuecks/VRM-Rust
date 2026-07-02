---
name: fix_audit
description: Resolve every issue in a technical-audit.md — implement fixes by severity, update docs, and validate builds pass.
runAs: subagent
---

Your goal is to resolve every issue documented in `docs/technical-audit.md` while preserving existing functionality and overall architecture.

## Required Preparation
Before making any code changes, thoroughly read:
- `docs/architecture.md`
- `docs/modules.md`
- `docs/data-flow.md`
- `docs/technical-audit.md`

## Implementation Requirements
Address all issues in order of severity: Critical → High → Medium → Low.

For every fix:
- Preserve existing behavior unless a change is explicitly required.
- Avoid introducing regressions.
- Follow the project's existing coding conventions and architectural patterns.
- Keep implementations simple, maintainable, and well-structured.
- Remove obsolete code when no longer needed.
- Refactor related code where necessary, but avoid unnecessary large-scale rewrites.

## Documentation Maintenance
After each logical set of changes, verify whether architecture, module structure, or data flow has changed. If so, immediately update:
- `docs/architecture.md`
- `docs/modules.md`
- `docs/data-flow.md`

These must always reflect the current implementation.

## Validation
Before considering the task complete:
- Ensure all issues in scope have been resolved.
- Verify the project builds successfully (`cargo build`).
- Run all available tests (`cargo test`).
- Fix any newly introduced warnings or errors.
- Ensure no existing functionality has been unintentionally broken.

## Completion Criteria
- All issues from `docs/technical-audit.md` resolved according to severity.
- Project builds without errors.
- All tests pass.
- Implementation is clean, maintainable, and consistent with existing architecture.
- `docs/architecture.md`, `docs/modules.md`, `docs/data-flow.md` are synchronized with the implementation.
