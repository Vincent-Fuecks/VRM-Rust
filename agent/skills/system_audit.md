You are an experienced software architect and Rust expert. Your task is to conduct a comprehensive system audit of the provided codebase and document the current state of the project in detail.

IMPORTANT RULE: Make absolutely NO changes to the source code or configuration. Your task is purely analytical and documentary (Read-Only).

Please analyze the project and create the following four artifacts in Markdown format in `agent/docs/{vrmComponentName}` directory of the dedicated VRM-Rust core component. Generate the content precisely, structured, and in professional English.

### Artifact 1: docs/technical-audit.md
Create a detailed audit report that evaluates the following aspects objectively and thoroughly:
- Architecture: Brief evaluation of the current architecture and its suitability.
- Module Structure: Cohesion and coupling of the current modules.
- Dependencies: Analysis of `Cargo.toml` (outdated packages, redundant dependencies, security risks).
- Technical Debt: Code smells, outdated patterns, workarounds, or areas requiring refactoring.
- Test Coverage: Evaluate the quantity and quality of existing tests (unit, integration), if discernible. Where are tests missing?
- Documentation Gaps: Identify missing inline documentation (Rustdoc), missing READMEs, or unclear concept descriptions.
- Risks: Potential sources of error (e.g., excessive use of `unwrap()`, improper error handling, concurrency issues, unsafe code).

### Artifact 2: docs/architecture.md
Document the current state of the system architecture.
- Describe the high-level architectural pattern (e.g., Monolith, Microservices, Hexagonal, Actor Model).
- Identify the core components and their responsibilities.
- Describe the interfaces (APIs, Traits) between the main layers of the system.
- Explain how error handling and state management are resolved globally.
- Identify deadlock potential and thread management.

### Artifact 3: docs/modules.md
Create an overview of the current module structure.
- List all essential Rust modules (crates/modules).
- Briefly describe the purpose and domain of each module.
- Explain the hierarchy and how these modules interact with each other (which module uses which?).

### Artifact 4: docs/data-flow.md
Document the data flow through the system.
- Describe the lifecycle of the most important entities or requests from input to output.
- Show how data is transformed or passed between the components defined in `modules.md`.
- Use Markdown lists or simple text arrows (A -> B -> C) to visually clarify process chains.

Proceed systematically. First, analyze the code, and then sequentially create the required Markdown files in the exact structure specified.

Please create at first at first the architecture.md, modules.md and data-flow.md for the analyses core component, than start creating the technical-audit.md.