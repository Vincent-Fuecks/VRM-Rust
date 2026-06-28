# Business Logic & Workflow Analysis: Schedule Component

## 1. Executive Summary

The Schedule component provides a robust architectural foundation for time-slotted resource scheduling. The strategy pattern, sliding window, and fragmentation caching are well-conceived. However, the business logic layer contains **several conceptual gaps and contradictions** that could lead to incorrect scheduling decisions in edge cases, silent data corruption, and unpredictable behavior under production workloads. The most critical concerns center on:

- **TOCTOU race conditions** between probe and reserve (the probe answer can be stale by the time reserve is called).
- **Inconsistent rejection semantics** — `reserve()` rejects silently via `None` but other paths set `ReservationState::Rejected`; callers can observe conflicting states.
- **Capacity reduction eviction is non-deterministic and incomplete** — iterating a `HashSet` in arbitrary order means different reservations get evicted each run, and the algorithm may stop prematurely, leaving a slot in an over-capacity state.
- **Unimplemented LinkStrategy analytics** return sentinel values (`-1.0`, `LoadMetric(-1,…)`) that could be silently consumed by downstream metrics pipelines, corrupting reporting.
- **Reservation abandonment** has no timeout or cleanup mechanism — reservations left in `ProbeReservation` or intermediate states persist indefinitely, leaking capacity and polluting the `ReservationStore`.

## 2. Business Logic Flaws & Loopholes

### [LOGIC-01]: TOCTOU Gap Between Probe and Reserve — Stale Feasibility

* **Business Impact:** High — A reservation that passes `probe()` can fail at `reserve()` time because capacity was consumed by another concurrent or interleaved reservation. The system has no mechanism to detect or communicate this.
* **The Scenario:** 
  1. Scheduler A calls `probe(reservation_X)` and receives feasible candidates.
  2. Before Scheduler A calls `reserve(reservation_X)`, Scheduler B calls `reserve(reservation_Y)` which books the same slots.
  3. Scheduler A calls `reserve(reservation_X)`. The `reserve()` method re-probes (calls `calculate_schedule()` again), and may find no feasible slots, returning `None`.
  4. The caller only receives `None` — there's no differentiation between "never feasible" and "was feasible but stolen by a concurrent reservation."
* **The Flaw:** The probe phase commits temporary reservations to compute fragmentation deltas (in `probe()`, lines where `reserve_without_check` is called then immediately deleted), but these are local and invisible to other schedules. The reserve phase re-probes from scratch. There is no reservation-hold or lock mechanism during the probe→reserve window. A caller that iterates through probe candidates thinking they are all valid will encounter failures.
* **Recommended Business Rule Change:** 
  - Introduce a **soft-reservation** or **intent-lock** concept: after `probe_best()` selects a candidate, call `reserve()` with that specific candidate using an optimistic lock. If the slots are no longer available, return a distinct error variant (e.g., `ReservationResult::CapacityStolen`) rather than generic `None`.
  - Alternatively, clearly document the TOCTOU nature of the API and provide a `probe_and_reserve_atomic()` method that performs probe+reserve atomically (which is effectively what `reserve()` already does, but the API should make this distinction clearer).

---

### [LOGIC-02]: Capacity Reduction (`update_capacity`) Produces Non-Deterministic and Potentially Incomplete Eviction

* **Business Impact:** High — When physical capacity is reduced (e.g., a node's cores are scaled down), the eviction logic produces different results on every run and can leave slots in an illegal over-capacity state.
* **The Scenario:**
  1. A slot has capacity=10, load=8, and 3 reservations: A (3 cores), B (3 cores), C (2 cores).
  2. `update_capacity(6)` is called. The slot's load (8) exceeds the new capacity (6).
  3. The code iterates over `slot.reservation_ids.clone()` — a `HashSet` with **non-deterministic iteration order**.
  4. If A is evicted first → load becomes 5 → slot.capacity is set to 6, and the loop breaks. Reservations B and C survive.
  5. If C is evicted first → load becomes 6 → still ≥ 6, so eviction continues. Then if B is evicted → load becomes 3 → break. A survives.
  6. **Different runs produce different surviving reservations.**
* **The Flaw:** 
  - The eviction order is arbitrary (HashSet iteration). There is no business rule defining which reservations should be sacrificed first — e.g., lowest priority, shortest remaining duration, newest first.
  - The loop breaks as soon as `slot.load < new_capacity`, but it **does not set `slot.capacity = new_capacity` if eviction fails to reduce load enough**. If all reservations are evicted and `slot.load` is still ≥ `new_capacity` (impossible in practice since load comes only from reservations, but structurally possible), the slot retains its old capacity, creating an inconsistency.
  - Evicted reservations are **only removed from the slot** — they are NOT removed from `active_reservations` and their `ReservationState` is NOT updated to `Rejected`. This means the reservation still "exists" but has been partially stripped from slots — a zombie reservation.
* **Recommended Business Rule Change:**
  - Define an explicit eviction policy: e.g., evict shortest-duration-first, or lowest-priority-first, or newest-first.
  - After evicting a reservation from all its slots, update its state to `ReservationState::Rejected` and remove it from `active_reservations`.
  - After all evictions, verify that every slot's load ≤ new capacity. If not, log a critical error and force-set capacity with all reservations evicted.
  - Consider whether `update_capacity` should be an atomic operation — the current incremental approach can leave the schedule in an inconsistent intermediate state if the method panics partway through.

---

### [LOGIC-03]: Inconsistent Rejection State Management Across Code Paths

* **Business Impact:** Medium — The `ReservationState` of a reservation can become inconsistent depending on which code path handles the rejection. Downstream consumers of reservation state see contradictory information.
* **The Scenario:**
  1. `probe()` sets `ReservationState::ProbeAnswer` after calculating candidates — even if candidates is empty. So an empty probe result still says "ProbeAnswer."
  2. `reserve()` sets `ReservationState::Rejected` when `calculate_schedule` returns empty, but also sets it on negative capacity.
  3. `reserve_without_check()` sets `ReservationState::ReserveAnswer` on success, but on negative capacity (early stop) sets `Rejected` — then **still continues** to insert into slots (because there's no `return` after setting Rejected).
  4. `delete_reservation()` (on `SlottedScheduleContext`) does not set any state after deletion. The reservation remains in whatever state it was.
* **The Flaw:** There is no unified state machine guarding transitions. A reservation can be simultaneously `Rejected` (set by early stop in `reserve_without_check`) and `ReserveAnswer` (set at end of the same method). The `ReservationStore` is updated with `Rejected` early but the method continues executing and sets `ReserveAnswer` at the end — a contradiction.
* **Recommended Business Rule Change:**
  - Define a strict state machine with valid transitions: `Initial → ProbeRequest → ProbeAnswer → ReserveRequest → ReserveAnswer (Committed) | Rejected`.
  - Add a `return` after setting `Rejected` in all early-stop branches of `reserve_without_check()`.
  - Add a guard that prevents setting `ReserveAnswer` if the state is already `Rejected`.
  - On `delete_reservation`, transition state to `Deleted` or `Cancelled`.

---

### [LOGIC-04]: Negative Capacity Reservations Pass Through Without Proper Rejection

* **Business Impact:** High — A reservation with negative reserved capacity triggers an error log and state update but the calling code **continues executing** as if nothing happened, potentially corrupting slot loads and capacity accounting.
* **The Scenario:**
  1. `reserve_without_check()` is called with a reservation that has `get_reserved_capacity() < 0`.
  2. It logs an error and sets `ReservationState::Rejected`.
  3. **It does not return.** Execution continues to the `for slot_index in start_slot..=end_slot` loop, calling `S::insert_reservation_into_slot()` with a negative requirement.
  4. In `Slot::insert_reservation()`, `self.load + requirement > self.capacity` is `load + (-X) > capacity`, which for small negative values will be `false` — the insertion proceeds, **reducing** the slot's load (effectively freeing capacity that was never reserved), and inserting the reservation ID into the slot's tracking set.
* **The Flaw:** The early-stop guard in `reserve_without_check()` is missing a `return` statement. The same pattern exists in `reserve()` and `probe()` — they set `Rejected` but then continue to `update()` and `calculate_schedule()`, though in those cases the subsequent behavior may be benign (empty results).
* **Recommended Business Rule Change:**
  - Add an immediate `return` after every early-stop rejection in `reserve_without_check()`, `reserve()`, `probe()`, and `probe_best()`.
  - Add an invariant check in `Slot::insert_reservation()`: reject (return `false`, log error) any insertion where `requirement <= 0`.

---

### [LOGIC-05]: Reservation Abandonment — No Cleanup for Orphaned Probe/Intermediate Reservations

* **Business Impact:** Medium — Over time, reservations left in `ProbeReservation` or intermediate states accumulate in the `ReservationStore`, consuming memory and potentially skewing metrics that iterate over all reservations.
* **The Scenario:**
  1. A scheduler calls `probe()` and receives candidates. It decides not to proceed with `reserve()`.
  2. The intermediate probe candidates were added to `ReservationStore` via `add_probe_reservation()`, and temporary reservations were made and deleted in the fragmentation calculation loop.
  3. However, any probe reservation that was added but not cleaned up remains. There is no garbage collection or TTL for probe reservations.
* **The Flaw:** The system has no concept of a reservation lease or timeout. If a client abandons the workflow after probing, the probe artifacts remain forever. In long-running simulations, this is a memory and performance leak.
* **Recommended Business Rule Change:**
  - Implement a TTL (time-to-live) for reservations in `ProbeReservation` state. After TTL expires, automatically delete them.
  - Provide a `cleanup_stale_probes()` method that callers (or a background task) can invoke periodically.
  - Alternatively, don't persist probe candidates to the `ReservationStore` at all — compute them ephemerally and only persist when `reserve()` commits.

---

### [LOGIC-06]: LinkStrategy Returns Sentinel Values for Unimplemented Analytics — Corrupts Downstream Decisions

* **Business Impact:** High — Scheduling decisions, load-balancing, and capacity planning algorithms that consume fragmentation and load metrics will receive **silently wrong values** for network schedules, leading to incorrect business decisions.
* **The Scenario:**
  1. A load-balancing algorithm queries `get_system_fragmentation()` on a `LinkStrategy` schedule. It receives `-1.0`.
  2. The algorithm interprets `-1.0` as "perfectly unfragmented" or "invalid" — either way, it makes a placement decision based on garbage data.
  3. Similarly, `get_load_metric()` returns `LoadMetric { start_time: -1, end_time: -1, avg_reserved_capacity: -1.0, possible_capacity: -1.0, utilization: 0.0 }`. A utilization of `0.0` suggests the network is idle when it may be saturated.
* **The Flaw:** Returning sentinel/magic values that could pass through numeric computations undetected creates a **silent data corruption** risk. Compare with returning `0.0` for fragmentation (which at least is a valid score, meaning "best"), `-1.0` is completely out of band.
* **Recommended Business Rule Change:**
  - Change LinkStrategy stubs to return `0.0` for fragmentation (no information = assume best case, with a warning log) and a zeroed `LoadMetric` with utilization `0.0`.
  - Alternatively, use `Option<f64>` / `Option<LoadMetric>` return types for the `Schedule` trait methods, so callers can explicitly handle "not available."
  - Short-term: add a `is_implemented()` capability query to the trait so callers can check before relying on analytical methods.
  - **Note:** The technical-audit states this was "resolved" by returning safe defaults but the live code (`link_strategy.rs`) still returns `-1.0` and `LoadMetric::new(-1, -1, -1.0, -1.0, 0.0)` — the fix was documented but not actually applied.

---

### [LOGIC-07]: `reserve()` Decision Logic Uses EST Comparator Implicitly, Ignoring Fragmentation

* **Business Impact:** Medium — The `reserve()` method always uses `ProbeReservationComparator::ESTReservationCompare` (Earliest Start Time) to select the winning candidate, ignoring fragmentation impact. This optimizes for packing but may cause severe fragmentation over time.
* **The Scenario:**
  1. `probe()` expensively computes fragmentation deltas for each candidate.
  2. `reserve()` calls `calculate_schedule()` (re-probes) and then `only_promote_best()` with `ESTReservationCompare`.
  3. The fragmentation deltas computed during `probe()` are **completely ignored** by `reserve()`. The EST comparator may choose a candidate that maximizes fragmentation, undoing the value of the fragmentation-aware probing.
* **The Flaw:** There is a disconnect between the intelligence in `probe()` (which can rank by fragmentation) and the decision in `reserve()` (which always ranks by EST). If fragmentation-aware scheduling is a business requirement, the current `reserve()` flow undermines it.
* **Recommended Business Rule Change:**
  - `reserve()` should accept an optional `ProbeReservationComparator` parameter, or default to a configurable comparator (not hardcoded EST).
  - Alternatively, `probe_best()` already supports a comparator — consider making `reserve()` delegate to `probe_best()` + commit, rather than re-calculating.
  - If EST is the intended default for `reserve()`, document that `probe_best()` with a custom comparator should be used when fragmentation-aware decisions are needed, and then the result passed to `reserve_without_check()`.

---

### [LOGIC-08]: Deletion of Already-Finished Reservation Silently Returns (No State Update)

* **Business Impact:** Low-Medium — When `delete_reservation()` is called on an already-finished reservation, it silently returns without updating the reservation state or notifying the caller. The caller cannot distinguish "already finished" from "successfully deleted."
* **The Scenario:**
  1. Reservation A has `assigned_end <= current_time` (it finished).
  2. A scheduler calls `delete_reservation(A)`.
  3. The `SlottedScheduleContext::delete_reservation()` checks `task_finished` and returns early with only an error log.
  4. The caller receives no return value indicating success or failure — the method returns `()`.
  5. The reservation remains in `ReservationState::ReserveAnswer` (or whatever state) rather than transitioning to a terminal state.
* **The Flaw:** This is a process gap: there is no concept of "archiving" or "finalizing" completed reservations. Once a reservation's end time passes, it becomes immutable but also invisible to most operations. A caller trying to clean up a completed reservation has no way to do so.
* **Recommended Business Rule Change:**
  - Change `delete_reservation` to return a `Result<(), DeletionError>` so callers can distinguish "not found," "already finished," and "success."
  - For already-finished reservations, transition state to `Finished` (or `Archived`) rather than returning silently.
  - Consider whether finished reservations should be moved to a separate historical store rather than remaining in the active `ReservationStore`.

---

### [LOGIC-09]: Moldable Reservation Capacity Adjustment May Cause Infinite or Excessively Long Loops

* **Business Impact:** Medium — When a moldable reservation has its capacity reduced to fit available space, its duration may **increase**. This can push the end time past the scheduling window or booking interval boundary, causing the candidate to be rejected. However, the loop continues to the next slot start time, potentially examining thousands of infeasible start times.
* **The Scenario:**
  1. A moldable reservation with capacity=10, duration=1h is probed. At slot N, only capacity=5 is available.
  2. `candidate.adjust_capacity(5)` is called. The moldable reservation may increase its duration to 2h (e.g., half the cores → double the time).
  3. `end_time = start_time + new_duration`. If this now exceeds the booking interval end or scheduling window, the candidate is rejected.
  4. This repeats for **every subsequent start slot** in the loop range, all of which will similarly fail because the capacity constraint is the same. The loop does not detect this pattern.
* **The Flaw:** The probe iterates linearly through all possible start slots even when the capacity constraint makes all of them infeasible after moldable adjustment. For large scheduling windows, this wastes significant computation.
* **Recommended Business Rule Change:**
  - After a moldable adjustment fails, check if the failure reason was duration overflow. If so, skip ahead by the adjusted duration before continuing the loop.
  - Add a maximum iteration guard or early termination heuristic when consecutive failures exceed a threshold.

---

### [LOGIC-10]: Fragmentation Resubmit Method Has Inverted Rejection/Success Logic

* **Business Impact:** Medium — The `get_fragmentation_resubmit()` method in `fragmentation.rs` appears to have **inverted logic** in its success/rejection handling, producing incorrect fragmentation scores.
* **The Scenario:**
  1. The method iterates while `remaining_capacity > 0`, picking random active reservations and attempting `test_schedule.reserve(id)`.
  2. `reserve()` returns `Some(id)` on **success** (reservation committed).
  3. The code handles `Some(id)` by subtracting capacity and **adding** to `rejected_capacity`:
     ```
     Some(id) => {
         remaining_capacity -= ...;
         rejected_capacity += ...;  // BUG: this was a SUCCESS, not a rejection
     }
     ```
  4. `None` (rejection) subtracts capacity but does NOT add to `rejected_capacity`:
     ```
     None => {
         remaining_capacity -= ...;
         // missing: rejected_capacity += ...
     }
     ```
* **The Flaw:** The success and rejection branches are **swapped**. Successful re-bookings are counted as rejections and vice versa. This means the RFI (Resubmission Fragmentation Index) is computed as `rejected_capacity / total_free_capacity` with `rejected_capacity` actually tracking **successfully re-booked** capacity — the exact opposite of what it should measure.
* **Recommended Business Rule Change:**
  - Swap the logic: `Some(id)` → subtract from remaining, do NOT add to rejected. `None` → subtract from remaining, ADD to rejected.
  - Verify against the documented definition: "ratio of rejected capacity to total free capacity."

---

### [LOGIC-11]: `reserve_without_check()` Cleanup Loop Does Not Remove from Active Reservations on Partial Failure

* **Business Impact:** Medium — If `reserve_without_check()` fails to insert into `active_reservations` (because the ID already exists), it attempts to roll back all slot insertions. However, if the rollback itself fails, the reservation is left partially booked — it exists in some slots but not in `active_reservations`.
* **The Scenario:**
  1. `reserve_without_check(res_X)` inserts into slots [5, 6, 7] successfully.
  2. `active_reservations.insert(res_X)` returns `false` (duplicate).
  3. The cleanup loop attempts to delete from slots [5, 6, 7]. Slot 5 succeeds, but slot 6 fails (e.g., the slot's reservation_ids set doesn't contain res_X due to a prior partial cleanup).
  4. The error is logged, but execution continues to the next slots. The reservation ends up in an ambiguous state: partially in some slots, not in active_reservations, state set to `ReserveAnswer`.
* **The Flaw:** There is no transactional boundary. Partial failure during rollback is not recoverable.
* **Recommended Business Rule Change:**
  - Before inserting into slots, validate that the reservation ID is not already in `active_reservations` and that the reservation is in a valid state for commitment.
  - Consider a two-phase approach: validate all slots first, then commit. If any slot fails during validation, abort the entire operation before any mutations.
  - If rollback is attempted and fails, set the reservation state to an error state and log a critical alert.

---

### [LOGIC-12]: Simulation Load Metrics Slice Calculation May Underflow

* **Business Impact:** Low-Medium — The `get_simulation_load_metric()` calculation assumes the effective range (first + `SLOTS_TO_DROP_ON_START` .. last - `SLOTS_TO_DROP_ON_END`) is valid. If too few slots have been recorded, the index math can produce negative or inverted ranges.
* **The Scenario:**
  1. A simulation has just started. Only 2 slots have been recorded in the `LoadBuffer`.
  2. `SLOTS_TO_DROP_ON_START` = 5, `SLOTS_TO_DROP_ON_END` = 5.
  3. `index_of_first_slot = 0 + 5 = 5`, `index_of_last_slot = 1 - 5 = -4`.
  4. `get_effective_overall_load()` receives `start=5, end=-4`, which is an inverted range.
* **The Flaw:** There is no guard against the case where the number of recorded slots is less than the total drop slots. This could produce nonsense load metrics during early simulation stages.
* **Recommended Business Rule Change:**
  - Add a guard: if `index_of_last_slot <= index_of_first_slot`, return a `LoadMetric` indicating "insufficient data" (e.g., utilization = 0.0 with a warning log).
  - Alternatively, clamp `SLOTS_TO_DROP_ON_START` and `SLOTS_TO_DROP_ON_END` to `max(0, min(configured, total_slots / 4))`.

---

## 3. Unresolved Product Questions

* **What is the intended eviction priority when capacity is reduced?** The current implementation uses arbitrary HashSet iteration order. Should it be: newest-first, shortest-duration-first, lowest-priority-first, largest-reservation-first, or something else?
* **Should `reserve()` accept a comparator parameter** like `probe_best()` does, or is EST always the correct default for commitment decisions? The fragmentation delta computation in `probe()` suggests fragmentation-aware decision-making is important, but `reserve()` ignores it.
* **What is the intended behavior when `reserve_without_check()` is called on a reservation that is already committed?** Currently it attempts rollback on duplicate detection, but this is fragile. Should it be a no-op, an error, or an idempotent operation?
* **Should the LinkStrategy fragmentation and load metrics be prioritized for implementation**, or is it acceptable that network schedules operate without these analytics? The current sentinel values could silently corrupt any system that aggregates metrics across node and link schedules.
* **What is the desired lifecycle for completed (expired) reservations?** They are currently left in the `ReservationStore` and silently ignored by `delete_reservation()`. Should they be: archived to a separate store, automatically deleted after a grace period, or retained indefinitely for historical analysis?
* **Is the fragmentation resubmit algorithm's inverted success/rejection logic intentional**, or is it a bug? If intentional, the documentation and method name are misleading.
* **Should there be a maximum number of probe iterations or a timeout?** When both booking interval boundaries are unset (i64::MIN → 0, i64::MAX → i64::MAX), the probe loop could theoretically iterate over an enormous range if the scheduling window is large.
* **What happens when `update_capacity` is called concurrently on multiple threads?** The `Schedule` trait requires `Send + Sync`, but `SlottedScheduleContext` is not `Sync`. If two threads share a schedule via `Arc<Mutex<>>`, this is fine, but the architecture doc states "each schedule context is owned by a single scheduler thread" — what if that assumption is violated?
