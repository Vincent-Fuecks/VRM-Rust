VRM-Rust: Spec-Driven & Test-Driven Workflow

To maintain the highest quality standards (0% structural debt, high test coverage), all new features and modules must be implemented using a strict Spec-Driven and Test-Driven Development (TDD) loop.

1. Spec-Driven Initiation

Do not write business logic based on vague prompts.

Features must be described in a specification markdown file (e.g., specs/feature_name.md) containing clear Acceptance Criteria.

The AI agent must read the specification file before writing any code.

2. The TDD Loop

Whenever a new feature or specification is provided, the AI must follow these exact steps in order:

Write Tests First: Based on the Acceptance Criteria, write the integration tests (in the tests/ directory) or unit tests (in the src/ module).

Verify Failure: The AI must acknowledge that running cargo test at this stage should fail (since the logic isn't implemented yet).

Implement Logic: Implement the Rust business logic in the src/ directory to satisfy the tests. Ensure the code adheres to all architectural and coding guidelines.

Run Quality Gates: Ensure cargo test, cargo clippy, and cargo fmt pass without errors or warnings.

Refactor: Once the tests pass, evaluate the code for performance and idiomatic Rust structure. Refactor if necessary while ensuring tests remain green.

3. The Harness Correction Rule

If during implementation the AI produces code that passes tests but violates the architectural guidelines (e.g., using shared mutable state instead of the repository pattern), the user should NOT manually fix the code. Instead, the user will update the .harness/ files to explicitly forbid the bad pattern, and the AI will regenerate the code based on the updated harness.