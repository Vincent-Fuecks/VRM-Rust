# Your Improvement Suggestions

## Development Process Improvements
- When performing a batch of fixes from a technical audit, use a checklist approach: tackle critical/medium/minor in order, update the audit doc in the same cycle as the code changes.
- When editing files that use `RouterId` (which is a non-Copy type from `slotmap`), be careful with pattern matches that move the value — use `ref` to borrow, or clone explicitly.
- For batch edits across many files, use `single_find_and_replace` for targeted, minimal changes, and `edit_existing_file` for larger restructuring. Avoid editing files without reading them first.
