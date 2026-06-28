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
