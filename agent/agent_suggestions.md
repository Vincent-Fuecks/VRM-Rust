# Your Improvement Suggestions


## 1. Prefer `single_find_and_replace` over `edit_existing_file` for Documentation Updates

**Date:** 2025-07-17
**Status:** ✅ Resolved — codified into always-applied rule `File Editing Safety Protocol`.

**Problem:** The `edit_existing_file` tool with the `changes` parameter uses a diff-like DSL with `// ... existing code ...` placeholders. This DSL fails unpredictably when the replacement block contains formatted text (tables, backtick-delimited code spans, bold markers). After a failure, subsequent tool calls in the same cycle also degrade — the XML parser appears to lose parameter bindings, producing errors like `"string old_string is required"` even when the parameter is present.

**Observed errors:**
- `"Failed to edit file. To continue working with the file, read it again to see the most up-to-date contents"` — the DSL could not anchor to the surrounding context.
- `"Cannot read properties of undefined (reading 'trim')"` — cascading parser corruption.
- `"Tool  not found"` with empty tool name — parser lost the `name` attribute.

**Solution:** Use `single_find_and_replace` for all documentation edits. It uses literal exact-string matching (no DSL), making it robust against formatted content. The trade-off is that it requires case-exact strings with precise whitespace, but this is easily satisfied by copy-pasting from a prior `read_file` call.

**Guideline:**
1. Always `read_file` immediately before editing to get the exact current content.
2. Copy the exact substring to replace (including indentation and newlines).
3. Use `single_find_and_replace` with `replace_all: false` for targeted edits.
4. Only use `edit_existing_file` for source code changes where the lazy-comment DSL is genuinely needed to reduce context in large files.

## 2. Avoid `create_new_file` as a Fallback for Existing Files

**Date:** 2025-07-17
**Status:** ✅ Resolved — codified into always-applied rule `File Editing Safety Protocol`.

**Problem:** When `edit_existing_file` failed, a natural fallback was to rewrite the entire file with `create_new_file`. This fails because the file already exists, and the tool correctly prevents accidental overwrites.

**Solution:** Stick with `single_find_and_replace` for incremental edits. If a complete rewrite is necessary, delete the file first via `run_terminal_command` (`rm <filepath>`), then use `create_new_file`. However, this should be a last resort since it loses incremental edit history.

## 3. Docs-First Context Gathering — Avoid Broad Source Exploration

**Date:** 2025-06-28
**Status:** ✅ Resolved — codified into always-applied rule `Docs-First Context Gathering`.

**Problem:** When assigned a task touching a specific VRM component (e.g., Workflow), the agent spent ~15+ `read_file` calls broad-scanning `src/`, `tests/`, and every sub-module before producing output. The project already has `agent/docs/{component}/architecture.md`, `data-flow.md`, and `modules.md` for each core component, plus a master `agent/docs/vrm_rust_architecture.md`. These docs describe the architecture, data flow, module responsibilities, and key design decisions — enough to understand the domain and write accurate test specifications.

**Observed waste:** Reading `workflow.rs` (500+ lines), `co_allocation.rs`, `dependency.rs`, `heft_sync_workflow_scheduler.rs` (400+ lines), `reservation.rs`, `reservation_store.rs` (600+ lines), `aci.rs`, `vrm_manager.rs`, `common.rs`, `rms_simulator.rs`, `rms_node_simulator.rs`, `rms_network_simulator.rs`, `rms_dto.rs`, and all test files — when the docs already covered the scheduling flow, co-allocation semantics, dependency types, and cross-RMS virtual chain design.

**Solution — 3-phase approach:**

1. **Phase 1 — Read the docs only** (≤ 5 reads):
   - `agent/docs/vrm_rust_architecture.md` (master architecture)
   - `agent/docs/{component}/architecture.md` (component architecture)
   - `agent/docs/{component}/data-flow.md` (data flow)
   - `agent/docs/{component}/modules.md` (module index)
   - Any existing `test_cases.md` or `technical-audit.md`

2. **Phase 2 — Identify gaps:** From the docs, determine which specific method signatures, enum variants, or struct fields are needed for the test spec. Write down a targeted list of ≤ 5 questions.

3. **Phase 3 — Targeted source reads** (≤ 5 reads): Only read the specific source files needed to answer the gaps. For example:
   - `reservation.rs` → for `ReservationState` enum variants
   - `dependency.rs` → for `DataDependency` / `SyncDependency` field types
   - `heft_sync_workflow_scheduler.rs` → for `schedule_dummy_dependency()` and `schedule_cross_rms_dependency()` signatures

**Expected savings:** ~80% reduction in context-gathering token spend (from ~15+ reads to ~10 reads, and the reads are smaller/more targeted).
