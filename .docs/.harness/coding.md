# VRM-Rust: Coding Guidelines & Style Guide
## Correctness
- Generate only valid Rust code.
- Preserve existing behavior unless a change is explicitly requested.
- Do not introduce breaking changes without justification.

## Ownership and Borrowing
- Prefer borrowing over ownership transfer when ownership is not required.
- Prefer &str over String for read-only string inputs.
- Prefer &[T] over Vec<T> for read-only collection inputs.
- Do not introduce a clone() unless it is required.
- Justify every introduced clone().
- Minimize heap allocations.
- Minimize unnecessary copies.

## Error Handling
- Represent recoverable failures with Result<T, E>.
- Use the ? operator for error propagation when appropriate.
- Use explicit error types instead of String errors.
- Do not ignore returned errors.
- Do not use unwrap() in production code unless correctness can be proven.
- Do not use expect() in production code unless correctness can be proven.
- Do not use panic!() in production code.

## Type System
- Model domain concepts with dedicated types when semantics matter.
- Prefer newtypes over primitive aliases for domain identifiers.
- Use the type system to prevent invalid states.
- Encode invariants in types whenever practical.

## Functions
- Give each function a single responsibility.
- Use descriptive function names.
- Keep functions focused and cohesive.
- Extract reusable logic into separate functions.

## Visibility
- Default to private visibility.
- Add pub only when external access is required.

## Pattern Matching
- Prefer exhaustive pattern matching.
- Handle all meaningful enum variants explicitly.
- Use wildcard arms only when ignored variants are intentionally irrelevant.

## Iterators
- Prefer iterator adapters when they improve clarity.
- Prefer iterator chains over manual collection processing when readability is maintained.
- Avoid imperative loops when iterator-based solutions are clearer.

## Unsafe Code
- Do not use unsafe.

## Concurrency
- Prefer ownership transfer over shared mutable state.
- Prefer message passing over shared mutable state.
- Avoid Arc<Mutex<T>> when simpler designs exist.
- Avoid Rc<RefCell<T>> when simpler designs exist.
- Minimize synchronization complexity.


# Deadlock Prevention & Diagnostics
- Use only parking_lot for Mutex and RwLock.
- Name all spawned threads descriptively.
- Keep lock duration minimal.
- Avoid nested locks.

# Logging Requirements
- Log unencapsulated lock activity at debug level (thread, resource, state).
- Add importend information for analysing the current system state at info level. 
- Add importent information for debugging at debug level.

## Memory and Performance
- Avoid premature optimization.
- Optimize only when there is evidence of a bottleneck.
- Avoid unnecessary allocations.
- Avoid unnecessary temporary objects.
- Do not sacrifice correctness for performance.
- Do not sacrifice readability for minor performance gains.

## Documentation
- Document every public Function and Struct.
- Document minor and major updates of the VRM-Rust core components in there dedicated `architecture.md` files to keep them up to date. 
- Document major updates to the overall VRM-Rust architecture in the `.harness/architecture.md` file.
- Add tests for newly introduced business logic in the folder `/tests` (structure should represent the struture in teh `/src` folder).
- Test successful execution paths.
- Test error paths.
- Test edge cases.
- Keep tests deterministic.
- Keep tests independent.

## Dependencies
- Do not add a dependency unless it provides clear value.
- Prefer well-maintained dependencies.
- Prefer widely adopted dependencies.
- Avoid dependencies that solve trivial problems.
- Minimize dependency count.

## Naming
- Use descriptive names.
- Avoid ambiguous names.
- Avoid unexplained abbreviations.
- Name types, functions, and variables according to their intent.

## Comments
- Explain why, not what.
- Document assumptions when relevant.
- Document constraints when relevant.
- Do not restate obvious code behavior.

## Idiomatic Rust
- Prefer Option<T> for optional values.
- Prefer Result<T, E> for recoverable failures.
- Prefer match for exhaustive branching.
- Prefer if let for single-pattern extraction.
- Prefer while let when repeatedly matching a pattern.
- Prefer traits over inheritance-like designs.
- Prefer composition over complex hierarchies.

## Quality Gates
- Generated code must compile.
- Generated code must pass formatting checks.
- Generated code must pass lint checks.
- Generated code must pass tests.
- Generated code must not contain unjustified unsafe.
- Generated code must not contain unjustified clone().
- Generated code must not contain unjustified panics.
- Generated code must remain maintainable.
- Generated code must remain readable.

## Priority Order
- Correctness
- Safety
- Readability
- Maintainability
- Testability
- Performance
- Conciseness