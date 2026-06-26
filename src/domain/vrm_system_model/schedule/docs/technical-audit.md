# Technical Audit: Schedule Component

**Date:** 2025-01-XX  
**Scope:** `src/domain/vrm_system_model/schedule/`  
**Version:** Current (pre-release)

---

## Component Overview

The **Schedule** component provides a trait-based abstraction for time-slotted resource scheduling. It implements a sliding-window slot mechanism for tracking physical resource capacity (compute cores, network bandwidth) over discrete time intervals. Two concrete strategies exist:

- **`NodeStrategy`**: Manages computational resource capacity (e.g., CPU cores on a compute node).
- **`LinkStrategy`**: Manages network bandwidth capacity across grid network topologies using K-shortest path routing.

### Module Structure

```
schedule/
├── mod.rs
├── schedule_trait.rs                 # Schedule trait (public interface)
├── slotted_schedule/
│   ├── mod.rs                        # Type aliases
│   ├── schedule_base.rs              # Schedule trait impl
│   ├── slot.rs                       # Slot (capacity tracking unit)
│   ├── slotted_schedule_context.rs   # Core data structure
│   ├── fragmentation.rs             # Fragmentation algorithms
│   └── strategy/
│       ├── mod.rs
│       ├── strategy_trait.rs         # SlottedScheduleStrategy trait
│       ├── node/
│       │   └── node_strategy.rs      # Node strategy
│       └── link/
│           ├── mod.rs
│           ├── link_strategy.rs      # Link/network strategy
│           └── topology.rs           # Network topology & pathfinding
```

---

## 🔴 Critical Findings

### C1. Unused `FRAGMENTATION_POWER` in `slotted_schedule_context.rs`

**File:** `slotted_schedule/slotted_schedule_context.rs:15`

The constant `const FRAGMENTATION_POWER: f64 = 2.0;` is defined but **never used** in this file. The same constant is correctly defined and used in `fragmentation.rs`. This creates confusion for maintainers.

**Severity:** Low (code clarity)  
**Recommendation:** Remove the unused constant from `slotted_schedule_context.rs`.

---

### C2. `reserve()` Return Value Documentation Inverted

**File:** `schedule_trait.rs:82-91` vs `schedule_base.rs:98-118`

The `Schedule::reserve()` trait method documents:
```
/// # Returns
/// `None` on success (reservation is accepted and committed),
/// or `Some(ReservationId)` if the ReservationId is rejected.
```

However, the implementation in `SlottedScheduleContext::reserve()` actually returns:
- `Some(reservation_id)` on **success** (capacity assigned)
- `None` on **failure** (rejected)

The documentation is **inverted** relative to the implementation.

**Severity:** High (API contract violation)  
**Recommendation:** Fix the trait documentation to state: `Some(reservation_id)` on success (committed/attempted), `None` on rejection.

---

### C3. LinkStrategy Unimplemented Methods Return Sentinel Values

**File:** `slotted_schedule/strategy/link/link_strategy.rs`

Four methods in `LinkStrategy` are stubs returning placeholder values:

| Method | Returned Value |
|--------|---------------|
| `get_fragmentation()` | `-1.0` |
| `get_system_fragmentation()` | `-1.0` |
| `get_load_metric()` | `LoadMetric::new(-1, -1, -1.0, -1.0, 0.0)` |
| `get_simulation_load_metric()` | `LoadMetric::new(-1, -1, -1.0, -1.0, 0.0)` |

The fragmentation value of `-1.0` is particularly dangerous: since valid fragmentation is `0.0` (best) to `1.0` (worst), a caller may not check for `-1.0` and could silently consume corrupted data.

**Severity:** High (data integrity risk)  
**Recommendation:** Either:
- (a) Implement the methods properly for LinkStrategy using multi-resource fragmentation calculation, or
- (b) Make `SlottedScheduleStrategy` trait methods return `Option<f64>` / `Option<LoadMetric>` to force callers to handle the unimplemented case, or
- (c) Panic with a clear error message indicating these methods are not yet implemented for LinkStrategy.

---

### C4. Sentinel Value Pattern for Time Boundaries

**File:** `slotted_schedule/slotted_schedule_context.rs`

The `calculate_schedule()` method checks for sentinel `i64::MIN` to detect unset booking intervals:

```rust
if request_start_boundary == i64::MIN { request_start_boundary = 0; }
if request_end_boundary == i64::MIN { request_end_boundary = i64::MAX; }
```

This is fragile — if `i64::MIN` is ever stored or propagated through calculations, it will produce silent incorrect results.

**Severity:** Medium  
**Recommendation:** Use `Option<i64>` instead of sentinel values.

---

## 🟡 Medium Findings

### M1. `unwrap()` / `expect()` Usage in Production Code

**Files:** Multiple locations in `slotted_schedule_context.rs`

```rust
// slotted_schedule_context.rs ~280
.slot.get_mut(slot_index)
    .expect(&format!("In the SlottedSchedule id: {} was the slot...not found.", ...));

// slotted_schedule_context.rs ~190
self.reservation_store.get_reservation_snapshot(candidate_id.clone())
    .expect("ReservationStore snapshot should handle potential errors.");
```

The format-based `expect()` allocates a String on every invocation, even in cases where the error would never be triggered. The second case's message indicates the developer knows the error should be handled differently.

**Severity:** Medium (recoverable errors become panics)  
**Recommendation:** Replace with proper error propagation where feasible. At minimum, use `expect()` with a static string (no allocation) or an explicit `unwrap_or_else(|| { log::error!(...); ... })` pattern.

---

### M2. No Tests Exist

**Directories checked:** `tests/**/schedule*`, `tests/**/slotted_schedule*`

There are **zero tests** for the Schedule component — no unit tests for:
- `Slot` (capacity, insert, delete, reset)
- `SlottedScheduleContext` (update, capacity, slot indexing)
- Fragmentation calculations (quadratic mean, resubmit)
- `NodeStrategy` (probe, reserve, delete)
- `LinkStrategy` (path-based capacity checks)
- `Schedule` trait contract adherence

**Severity:** High (no regression protection)  
**Recommendation:** Add tests matching the source structure under `tests/domain/vrm_system_model/schedule/`.

---

### M3. `Slot.reservation_ids.clone()` in `update_capacity()`

**File:** `slotted_schedule/slotted_schedule_context.rs` in `update_capacity()`:

```rust
for res_in_slot in slot.reservation_ids.clone().iter() {
```

This clones the entire `HashSet` of reservation IDs for every over-capacity slot. While necessary for borrowing reasons (deleting from a set while iterating), there's no comment explaining this design decision.

**Severity:** Low (performance)  
**Recommendation:** Add a comment: `// Clone required because we mutate (delete from) the set while iterating.`

---

### M4. Duplicated Feasibility Logic

**File:** `slotted_schedule/slotted_schedule_context.rs`

The `try_fit_reservation()` method contains slot-feasibility logic (capacity checking, moldable capacity adjustment) that is duplicated across the probe/reserve flow. There is no shared helper `fn is_slot_feasible(...)` that can be reused.

**Severity:** Medium (maintainability)  
**Recommendation:** Extract a shared feasibility check function.

---

### M5. Infinite Range in `get_slot_index()` and `probe()` Search Space

**File:** `slotted_schedule/slotted_schedule_context.rs:163-170`

When `request_end_boundary == i64::MAX`, the probe iterates through slots until `latest_start_index == end_slot_index`. Combined with no early termination, this is safe only because of the scheduling window bounds (`get_effective_slot_index()` clips). However, if the scheduling window is large (e.g., 10K+ slots), the probe becomes expensive.

**Severity:** Low-Medium (performance)  
**Recommendation:** Consider adding iteration limits or a more efficient search strategy.

---

### M6. `i64` as f64 Conversion Precision Loss

**File:** `slotted_schedule/slotted_schedule_context.rs:163`

```rust
let index: i64 = (time as f64 / self.slot_width as f64).floor() as i64;
```

`i64` → `f64` conversion loses precision for values > 2^53. With Unix time in seconds, this is ~285 million years, making it safe for current use. However, integer division `time / self.slot_width` for non-negative time would be simpler and exact.

**Severity:** Low  
**Recommendation:** Use integer division: `time.div_euclid(self.slot_width)` or `time / self.slot_width` with a clamp for negative values.

---

## 🟢 Minor Findings

### m1. Typos

| Location | Current | Correct |
|----------|---------|---------|
| `strategy_trait.rs` param name | `requirment` | `requirement` |
| `node_strategy.rs` param name | `requirment` | `requirement` |
| `ProbeReservations` field | `probe_meta_data` | `probe_metadata` |

### m2. Unused Import

**File:** `slotted_schedule/slotted_schedule_context.rs`

```rust
use std::i64;
```
This import is unnecessary in Rust 2018+.

### m3. Commented-out Code

**File:** `slotted_schedule/strategy/link/link_strategy.rs:39-53`

A complete `adjust_start_end()` function is commented out. This adds noise to the source.

### m4. Hardcoded Fragmentation Power

**File:** `fragmentation.rs`

`FRAGMENTATION_POWER = 2.0` is hardcoded. Different resource types (CPU vs bandwidth) might benefit from different power values. Consider making it part of the strategy or configuration.

### m5. Atomic Ordering Not Documented

**File:** `load_buffer.rs` (`GlobalLoadContext`)

All atomic operations use `Ordering::Relaxed`. While likely correct for this use case (monotonic updates of min/max indices), there is no comment explaining why stronger ordering is unnecessary.

### m6. Method Name Inconsistency

The trait has both `probe()` and `probe_best()` but the `Reservations` method is named `only_prompt_best()` (typo: "prompt" instead of "probe").

---

## Architecture & Design Review

### Strengths

1. **Strategy Pattern via Generics**: Using `SlottedScheduleContext<S: SlottedScheduleStrategy>` with compile-time generics avoids `Box<dyn>` overhead while enabling extensibility.
2. **Fragmentation Caching**: The `is_frag_cache_up_to_date` flag prevents redundant O(n) fragmentation recalculations during probe operations.
3. **Sliding Window**: Circular buffer for slots is memory-efficient and well-suited to continuous simulation.
4. **Clear Separation**: `Slot` is a focused value object; `SlottedScheduleContext` manages orchestration; strategy implementations handle domain-specific logic.

### Weaknesses

1. **No Documentation**: All four documentation files (`architecture.md`, `data-flow.md`, `modules.md`, `technical-audit.md`) are empty.
2. **No Tests**: Zero test coverage.
3. **LinkStrategy Incomplete**: Key analytical methods are unimplemented stubs, limiting network-level capacity analysis.
4. **Doc/Code Divergence**: The `reserve()` contract is documented incorrectly, indicating a potential API trust issue.
5. **Sentinel over Option**: The codebase uses `i64::MIN`/`i64::MAX` sentinels instead of idiomatic `Option<i64>`.

---

## Recommendations (Prioritized)

### Phase 1 — Critical Safety
1. Fix `reserve()` documentation (C2)
2. Remove unused `FRAGMENTATION_POWER` constant (C1)
3. Implement or properly guard LinkStrategy unimplemented methods (C3)
4. Replace `i64::MIN`/`i64::MAX` sentinels with `Option<i64>` (C4)

### Phase 2 — Correctness & Maintainability
5. Add comprehensive tests (M2)
6. Replace panic-prone `unwrap()`/`expect()` calls (M1)
7. Fix method name `only_prompt_best` → `only_probe_best` (m6)
8. Extract shared feasibility logic (M4)
9. Add defensive slot-index bounds documentation (M3)

### Phase 3 — Polish
10. Fix typos (m1)
11. Remove unused import (m2)
12. Remove commented-out code (m3)
13. Document atomic ordering rationale (m5)
14. Make fragmentation power configurable (m4)
15. Use integer division for slot indexing (M6)

---

## File-by-File Summary

| File | Issues | Status |
|------|--------|--------|
| `schedule_trait.rs` | C2: reserve() doc inverted | ❌ |
| `slotted_schedule_context.rs` | C1: unused constant, C4: sentinel, M1: unwrap/expect, M3: clone, M4: logic dup, M6: float precision, m2: unused import | ❌ |
| `fragmentation.rs` | m4: hardcoded power | ⚠️ |
| `slot.rs` | None (clean) | ✅ |
| `strategy/strategy_trait.rs` | m1: typo `requirment` | ❌ |
| `strategy/node/node_strategy.rs` | m1: typo `requirment` | ❌ |
| `strategy/link/link_strategy.rs` | C3: unimplemented stubs, m3: commented-out code | ❌ |
| `strategy/link/topology.rs` | None (clean) | ✅ |

**Legend:** ✅ No issues | ⚠️ Minor issues | ❌ Issues to fix
