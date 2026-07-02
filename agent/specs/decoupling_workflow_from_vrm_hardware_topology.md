## WorkflowScheduler Overview
The **WorkflowScheduler** is a scheduler that receives a workflow as input. This workflow consists of a **dependency graph** made up of both **Data and Sync Dependencies**:

* **Data Dependencies:** These are dependencies where *Reservation A* can only start once it has received data from *Reservation B*.
* **Sync Dependencies:** These represent runtime communication dependencies between the respective reservations. For example, *Reservation A* needs to communicate with *Reservation B* during runtime.

Additionally, all dependencies are pre-scheduled into so-called **Co-Allocation Groups**. A Co-Allocation Group encompasses all reservations that must be scheduled simultaneously. 

The workflow consists of `workflowNodes`, which in turn contain references to the **Reservations**. Only the Reservations retain relevance for the VRM system further down the line. These Reservations have already been added to the `ReservationStore` during the workflow creation process.

The WorkflowScheduler receives [the workflow] and subsequently attempts to distribute the respective Co-Allocation Groups across the VRM system in the best possible way. During this process, it ensures that both the Sync Dependencies and the Data Dependencies of all Co-Allocation Groups are guaranteed.

The WorkflowScheduler is executed by the **ADC-Master**. The ADC-Master enables the workflow reservations to be split up and distributed among the underlying ADC components.

---

### Invariants

1. **Workflow distribution:** The workflow is distributed to the underlying ADC components. Only the ADC-Master handles reservations of the type `Workflow`. Consequently, all underlying components only work with `Node` and `Link` reservations.
2. **Access control:** Only the ACD-Master is permitted to use the `WorkflowManager`.
3. **Pre-flight checks:** Workflow reservations may only be reserved after a probe request has been executed for all workflow reservations and has completed successfully.
4. **Atomicity:** The reservation of the workflow and all of its connected reservations happens all at once (atomically).
5. **Fallback:** If a workflow cannot be scheduled, all of its associated reservations are set to `Rejected`.

---

## Feature: Decouple Workflow from VRM Hardware Topology

### Problem Statement

Currently the user must know and specify RMS node names when defining a workflow. If we only have `rms_0` with `node_1` and `node_2`, the user must specify for each computation task `node_1` or `node_2`. Analogously for link reservations. Additionally, it is not possible to schedule reservations that involve sending data **between** different RMS systems, because routing reservations across RMS boundaries is not yet implemented. In the future, AcI components will run directly on HPC sites where the total infrastructure is unknown to the central VRM system.

### Goal

Decouple the user-provided workflow from the actual VRM hardware topology. The topology must be hidden from the user. Gateway nodes abstract each RMS cluster behind a single entry/exit point, enabling cross-RMS data transfers without exposing internal topology.

---

## Resolved Architectural Decisions

### AD-1: Global Unified NetworkTopology at ADC Level

**Decision:** A single, unified, global `NetworkTopology` is maintained at the ADC-Master level. This topology encompasses all per-RMS topologies PLUS inter-gateway links connecting them. This enables the ADC-level `ResourceStore` to perform path-finding across RMS boundaries via a single k-shortest-paths cache.

**Scope:** `ResourceStore` at the ADC level gains a global topology that includes:
- All internal links/nodes from each connected RMS
- Per-RMS gateway nodes (see AD-2)
- Inter-gateway links connecting the gateways

### AD-2: Unique Per-RMS Gateway RouterIds, JSON-Configurable

**Decision:** Gateway RouterIds are unique per RMS (e.g., `"AcI-Gateway-rms_0"`, `"AcI-Gateway-rms_1"`). The gateway name is **configurable in the VRM JSON configuration file**, not hardcoded as a constant. The existing global constant `RMS_GATEWAY_NAME` is removed in favor of a per-RMS configuration field.

**Scope:**
- Remove `RMS_GATEWAY_NAME` constant from `src/vrm/common/config.rs`
- Introduce a separate configuration mechanism (see AD-9) for per-RMS gateway naming
- Update `get_nodes_and_links()` in `src/vrm/rms/common.rs` to accept a per-RMS gateway name parameter
- Update all tests and example JSON files

### AD-3: Cross-RMS Links as Virtual Reservation Chain

**Decision:** Cross-RMS data dependencies are split into a chain of **virtual link reservations** tracked in the `ReservationStore`. The path is:

```
source_node → source_gateway  (on AcI_0, internal link)
source_gateway → ADC-System   (virtual link at ADC level)
ADC-System → target_gateway   (virtual link at ADC level)
target_gateway → target_node  (on AcI_1, internal link)
```

All four segments are tracked as `LinkReservation`s in the `ReservationStore`. The virtual reservations (the two middle segments) are **only accessible by the original parent reservation** and are automatically deleted when the original reservation is deleted. This preserves the atomicity invariant: if any segment fails to schedule, all previously scheduled segments for that dependency are rolled back via `cancel_all_reservations()`.

**Scope:**
- Introduce "virtual reservation" concept in `ReservationStore` — reservations with a `parent_reservation_id` field
- Refactor `schedule_real_dependency()` in `HEFTSyncWorkflowScheduler` to split cross-RMS link scheduling into 4 segments
- Add cleanup logic: when a parent reservation is deleted/rejected, all linked virtual reservations are cascade-deleted

### AD-4: Gateway RouterIds as Cross-RMS Link Endpoints

**Decision:** Cross-RMS link reservations use **gateway RouterIds** as `start_point` / `end_point` (option B from review). Internal segments (source_node → source_gateway and target_gateway → target_node) are scheduled as separate reservations on the respective AcI components. The AcI-gateways are stored as resources in a dedicated ADC-level `ResourceStore` with input/output capacity limits modeled by a `NetworkSchedule`. A JSON configuration mechanism is introduced for gateway bandwidth limits.

**Scope:**
- ADC-level `ResourceStore` stores gateway nodes as `NodeResource`s with capacity = 0 (routing-only)
- Gateway input/output bandwidth limits are modeled via a `LinkStrategy`-based `SlottedScheduleContext` per gateway pair
- JSON schema extended for gateway bandwidth configuration (see AD-9)

### AD-5: `get_component_router_list()` is Deprecated (Information Hiding)

**Decision:** `get_component_router_list()` is legacy and will be **removed**. The VRM follows the **information hiding principle**: underlying infrastructure is hidden from higher system components. The ADC does not enumerate internal routers of child components. Instead, the ADC only knows each component's **gateway RouterId**. Internal routing within an RMS is the AcI's responsibility.

**Scope:**
- Remove `get_component_router_list()` from `VrmComponentManager`
- Remove calls to it from `HEFTSyncWorkflowScheduler::schedule_real_dependency()`
- Replace with gateway-based routing (see AD-3, AD-4)

### AD-6: Deeper Capacity Abstraction → Deferred to Separate US

**Decision:** The current capacity-based feasibility check (where the AcI checks if *any* internal node has sufficient capacity without revealing *which* node) is sufficient for this feature. Further hiding of capacity information behind the gateway is deferred to a separate User Story: `agent/specs/hide_capacity_information_behind_gateway.md`.

### AD-7: TDD — Comprehensive Test Coverage

**Decision:** The following test scenarios must pass:
1. **Single-RMS workflow**: 2+ tasks within one RMS, connected by data and sync dependencies → all schedule successfully
2. **Cross-RMS workflow**: Tasks on different RMS systems connected by data dependencies → link reservations split into virtual reservation chains, all schedule successfully
3. **Cross-RMS workflow with sync dependencies**: Co-allocated tasks spanning two RMS systems connected by sync dependencies
4. **Failure rollback**: When one RMS cannot satisfy its part of a cross-RMS dependency, all previously scheduled segments are rolled back and all reservations set to `Rejected`
5. **Virtual reservation cleanup**: When a parent workflow reservation is deleted, all virtual link reservations are cascade-deleted

### AD-8: Dual Path-Finding Strategy with Config Toggle

**Decision:** Both path-finding approaches are implemented with a boolean configuration flag in `src/vrm/common/config.rs`:

```rust
/// If true, use full multi-hop k-shortest-paths for inter-gateway routing.
/// If false, treat gateway-to-gateway as a single virtual resource with
/// capacity = min(ingress, egress bandwidth).
pub const USE_FULL_INTER_GATEWAY_PATH_FINDING: bool = false;
```

**Scope:**
- Default to `false` (simple single-hop virtual resource)
- When `true`, the global `NetworkTopology` must include intermediate routers/switches between gateways
- The simple mode creates a direct virtual link between gateway RouterIds with a `LinkResource` in the ADC-level `ResourceStore`

### AD-9: Separate Gateway Configuration Mechanism

**Decision:** A new, dedicated JSON configuration section is introduced for per-RMS gateway configuration. This is **separate** from the existing `TopologyDto` fields. The new schema provides:

```json
{
  "gatewayConfig": {
    "rms_0": {
      "gatewayRouterId": "AcI-Gateway-rms_0",
      "ingressBandwidthGbps": 1000,
      "egressBandwidthGbps": 1000,
      "gatewaySwitchId": "s0"
    },
    "rms_1": {
      "gatewayRouterId": "AcI-Gateway-rms_1",
      "ingressBandwidthGbps": 1000,
      "egressBandwidthGbps": 1000,
      "gatewaySwitchId": "s0"
    }
  },
  "interGatewayLinks": [
    {
      "sourceGateway": "AcI-Gateway-rms_0",
      "targetGateway": "AcI-Gateway-rms_1",
      "bandwidthGbps": 10000
    }
  ]
}
```

The existing `TopologyDto` fields (`gatewaySwitchId`, `ingressBandwidthGbps`, `egressBandwidthGbps`) remain for backward compatibility but the gateway RouterId is overridden by the new configuration when present.

**Scope:**
- New DTO types: `GatewayConfigDto`, `InterGatewayLinkDto`
- New configuration parsing in RMS setup
- Per-RMS gateway RouterId generation (with fallback to `format!("AcI-Gateway-{}", component_id)` if not explicitly configured)

---

## Implementation Checklist

### Phase 1: Foundation — Remove Legacy, Add Configuration
- [ ] Remove `RMS_GATEWAY_NAME` constant; remove `get_component_router_list()`
- [ ] Create `GatewayConfigDto` and `InterGatewayLinkDto` in `src/schema/`
- [ ] Update `TopologyDto` to reference new gateway config
- [ ] Update `get_nodes_and_links()` to accept per-RMS gateway name
- [ ] Add `USE_FULL_INTER_GATEWAY_PATH_FINDING` config flag

### Phase 2: Global Topology at ADC Level
- [ ] Build unified `NetworkTopology` at ADC level aggregating all RMS topologies + inter-gateway links
- [ ] Store gateway nodes as resources in ADC-level `ResourceStore`
- [ ] Model inter-gateway bandwidth via `LinkStrategy`-based `SlottedScheduleContext`

### Phase 3: Virtual Reservation Chain
- [ ] Add `parent_reservation_id: Option<ReservationId>` to `ReservationBase` for virtual reservation tracking
- [ ] Add cascade-delete logic: when parent is deleted, all virtual children are cleaned up
- [ ] Refactor `schedule_real_dependency()` to detect cross-RMS dependencies and split into 4-segment chains
- [ ] Update `cancel_all_reservations()` to handle virtual reservation rollback

### Phase 4: HEFT Scheduler Updates
- [ ] Replace `get_component_router_list()` calls with gateway-based routing
- [ ] Implement internal link scheduling (node→gateway) on source/target AcIs
- [ ] Implement virtual link scheduling (gateway→gateway) at ADC level
- [ ] Ensure atomicity: all 4 segments succeed or all are rolled back

### Phase 5: Tests
- [ ] Single-RMS workflow test (data + sync dependencies)
- [ ] Cross-RMS data dependency test
- [ ] Cross-RMS sync dependency (co-allocation) test
- [ ] Failure rollback test
- [ ] Virtual reservation cascade-delete test
- [ ] Full path-finding vs. single-hop config toggle test

### Phase 6: Documentation
- [ ] Update `agent/docs/vrm_rust_architecture.md`
- [ ] Update `agent/docs/workflow/` component docs
- [ ] Update `agent/docs/vrm_component/` architecture and data-flow docs

---

# Test Cases

## Category 1: Happy Paths — Single-RMS Workflows

### TC-1.1: Single-RMS Workflow — Two Tasks Within One RMS with Data Dependency

**Objective:** Verify that a workflow with 2+ node tasks residing on the same RMS, connected by a data dependency, schedules all reservations successfully without virtual reservation chains.

**Given:**
- A VRM system with a single RMS (`rms_0`) configured via `GatewayConfigDto` with `gatewayRouterId = "AcI-Gateway-rms_0"`, `ingressBandwidthGbps = 1000`, `egressBandwidthGbps = 1000`
- A `ReservationStore` containing a `Workflow` with two `NodeReservation`s (`task_A` on `rms_0::node_1`, `task_B` on `rms_0::node_2`), and one `LinkReservation` forming a data dependency from `task_A` → `task_B` (size > 0)
- Both node reservations are in `ReservationState::Open` with `ReservationProceeding::Commit`
- The `RMS_GATEWAY_NAME` constant has been removed; `get_component_router_list()` has been removed

**When:**
- `ADC.manager.can_handel(workflow_res_id)` is called (pre-flight probe)
- The result is `true`
- `HEFTSyncWorkflowScheduler::reserve(workflow_res_id, &mut adc)` is invoked

**Then:**
- `task_A` is scheduled successfully on `rms_0` with state `ReservationState::ReserveAnswer` or greater
- `task_B` is scheduled successfully on `rms_0` with state `ReservationState::ReserveAnswer` or greater
- The data dependency `LinkReservation` is scheduled successfully (same-RMS → handled as real or dummy dependency, not a virtual reservation chain)
- `grid_component_res_database` contains entries for all three reservation IDs mapped to `rms_0`'s component ID
- `workflow.state == ReservationState::ReserveAnswer`
- No virtual reservations (`parent_reservation_id != None`) are created in `ReservationStore`
- `store.original_to_virtual` is empty for this workflow

---

### TC-1.2: Single-RMS Workflow — Two Tasks with Sync Dependency (Co-Allocation)

**Objective:** Verify that a workflow with co-allocated tasks (sync dependency) on the same RMS schedules all members simultaneously.

**Given:**
- A VRM system with a single RMS (`rms_0`) configured with gateway config
- A `Workflow` with three `NodeReservation`s (`A`, `B`, `C`) all on `rms_0`
- Two `SyncDependency`s: `A → B` and `B → C` (forming `CoAllocation(A,B,C)`)
- All node reservations in `ReservationState::Open` with `ReservationProceeding::Commit`

**When:**
- `HEFTSyncWorkflowScheduler::reserve(workflow_res_id, &mut adc)` is invoked
- The scheduler processes the co-allocation group `{A, B, C}`

**Then:**
- All three node reservations (`A`, `B`, `C`) receive `assigned_start` values within the workflow booking interval
- All three node reservations reach at least `ReservationState::ReserveAnswer`
- The sync dependency `LinkReservation`s (`A→B`, `B→C`) are scheduled successfully
- Co-allocation members share consistent time windows (all have the same `assigned_start`)
- No cross-RMS virtual reservation chain is created
- `workflow.state == ReservationState::ReserveAnswer`

---

### TC-1.3: Single-RMS Workflow — Multiple Tasks, All on Same RMS, No Dependencies

**Objective:** Verify that a degenerate workflow (no dependencies) schedules all tasks independently on a single RMS.

**Given:**
- A VRM system with `rms_0`
- A `Workflow` with three independent `NodeReservation`s (no data or sync dependencies between them)
- All in `ReservationState::Open` / `ReservationProceeding::Commit`

**When:**
- `HEFTSyncWorkflowScheduler::reserve(workflow_res_id, &mut adc)` is invoked

**Then:**
- All three node reservations reach `ReservationState::ReserveAnswer`
- Each task receives its own `assigned_start` / `assigned_end`
- `workflow.state == ReservationState::ReserveAnswer`

---

## Category 2: Happy Paths — Cross-RMS Workflows

### TC-2.1: Cross-RMS Data Dependency — 4-Segment Virtual Reservation Chain

**Objective:** Verify that a data dependency from a node on `rms_0` to a node on `rms_1` is split into exactly 4 link reservation segments and all schedule successfully.

**Given:**
- A VRM system with two RMS components: `rms_0` and `rms_1`
- Gateway config:
  ```json
  {
    "rms_0": { "gatewayRouterId": "AcI-Gateway-rms_0", "ingressBandwidthGbps": 1000, "egressBandwidthGbps": 1000, "gatewaySwitchId": "s0" },
    "rms_1": { "gatewayRouterId": "AcI-Gateway-rms_1", "ingressBandwidthGbps": 1000, "egressBandwidthGbps": 1000, "gatewaySwitchId": "s1" }
  }
  ```
- An `InterGatewayLinkDto`: `{ "sourceGateway": "AcI-Gateway-rms_0", "targetGateway": "AcI-Gateway-rms_1", "bandwidthGbps": 10000 }`
- A `Workflow` with `task_A` on `rms_0::node_1` and `task_B` on `rms_1::node_2`
- A `DataDependency` from `task_A → task_B` with `size = 5000` (MB)
- `USE_FULL_INTER_GATEWAY_PATH_FINDING = false` (single-hop virtual resource mode)
- Both `task_A` and `task_B` have been scheduled via `schedule_node_reservation_eft` and mapped in `grid_component_res_database`

**When:**
- `HEFTSyncWorkflowScheduler::schedule_data_dependencies()` invokes `schedule_dependency()` for the cross-RMS data dependency
- The scheduler detects `source_component_id != target_component_id`

**Then:**
- Exactly 4 link reservations are created in `ReservationStore`:
  1. `source_node → AcI-Gateway-rms_0` (scheduled on `rms_0`'s AcI, internal link)
  2. `AcI-Gateway-rms_0 → ADC-System` (virtual link, `parent_reservation_id = Some(original_link_res_id)`)
  3. `ADC-System → AcI-Gateway-rms_1` (virtual link, `parent_reservation_id = Some(original_link_res_id)`)
  4. `AcI-Gateway-rms_1 → target_node` (scheduled on `rms_1`'s AcI, internal link)
- All 4 segments reach `ReservationState::ReserveAnswer`
- The original data dependency `LinkReservation` is updated via `workflow.update_reservation()` with correct `assigned_start` / `assigned_end`
- `store.original_to_virtual` has an entry mapping `original_link_res_id → [virtual_res_id_1, virtual_res_id_2]`
- Gateway RouterIds (`AcI-Gateway-rms_0`, `AcI-Gateway-rms_1`) are used as `start_point` / `end_point` on the virtual reservations

---

### TC-2.2: Cross-RMS Workflow — Sync Dependency Spanning Two RMS

**Objective:** Verify that a co-allocation group spanning two RMS systems schedules correctly, with sync dependencies split across RMS boundaries.

**Given:**
- A VRM system with `rms_0` and `rms_1`
- A `Workflow` with `task_A` on `rms_0::node_1` and `task_B` on `rms_1::node_2`, connected by a `SyncDependency A ↔ B`
- Both tasks co-allocated in the same `CoAllocation` group
- Both tasks are in `ReservationState::Open` / `ReservationProceeding::Commit`

**When:**
- `schedule_co_allocation_node_reservations()` schedules `task_A` first (on `rms_0`), then `task_B` (on `rms_1`)
- `schedule_sync_dependencies()` processes the sync dependency `A ↔ B` across RMS boundaries

**Then:**
- `task_A` is scheduled on `rms_0` and reaches `ReservationState::ReserveAnswer`
- `task_B` is scheduled on `rms_1` with matching `assigned_start` = `task_A.assigned_start` and reaching `ReservationState::ReserveAnswer`
- The sync dependency `LinkReservation` is split into a 4-segment virtual reservation chain (same as data dependency for cross-RMS)
- `is_filetransfer = false` is passed correctly; the link is scheduled as non-moldable
- `workflow.state == ReservationState::ReserveAnswer`

---

### TC-2.3: Cross-RMS Workflow — Three RMS Systems, Chain Dependency

**Objective:** Verify that a workflow with tasks on 3 distinct RMS systems (`rms_0 → rms_1 → rms_2`) correctly creates two independent 4-segment chains.

**Given:**
- VRM with `rms_0`, `rms_1`, `rms_2`
- Gateway config with unique gatewayRouterIds for all three
- `InterGatewayLinkDto` entries connecting `rms_0↔rms_1` and `rms_1↔rms_2`
- `Workflow` with `task_A` on `rms_0`, `task_B` on `rms_1`, `task_C` on `rms_2`
- `DataDependency A → B` and `DataDependency B → C`

**When:**
- `HEFTSyncWorkflowScheduler::reserve()` processes the workflow

**Then:**
- All three node reservations reach `ReservationState::ReserveAnswer`
- Two independent 4-segment virtual reservation chains are created (one for `A→B`, one for `B→C`)
- A total of 8 link reservations are in `ReservationStore` for the two dependencies
- `store.original_to_virtual` has 2 entries, each mapping to exactly 2 virtual reservations
- `workflow.state == ReservationState::ReserveAnswer`

---

## Category 3: Edge Cases & Boundaries

### TC-3.1: Dummy Dependency Across RMS Boundaries (Both Tasks on the Same RMS)

**Objective:** Verify that when a data dependency has both endpoints on the same RMS, it is treated as a dummy dependency (no virtual chain), regardless of the gateway abstraction.

**Given:**
- VRM with `rms_0` and `rms_1`
- `Workflow` with `task_A` on `rms_0::node_1` and `task_B` on `rms_0::node_2`
- `DataDependency A → B` where the scheduler assigned both to the same component

**When:**
- `schedule_dependency()` is called and detects `source_component_id == target_component_id`

**Then:**
- The dependency is handled by `schedule_dummy_dependency()` (if capacity is 0 or same component)
- State is set to `ReservationState::Committed` directly
- `start_point` / `end_point` are set to `"localhost"`
- No virtual reservations are created
- No entry in `store.original_to_virtual`

---

### TC-3.2: Zero-Byte Data Dependency Across RMS Boundaries

**Objective:** Verify that a data dependency with `size = 0` (no actual data transfer) is treated as a dummy dependency even when endpoints are on different RMS systems.

**Given:**
- VRM with `rms_0` and `rms_1`
- `Workflow` with `task_A` on `rms_0` and `task_B` on `rms_1`
- `DataDependency A → B` with `size = 0` but with `reserved_capacity = 0`

**When:**
- `schedule_dependency()` evaluates `reserved_capacity == 0 || source_component_id.compare(&target_component_id)`

**Then:**
- `schedule_dummy_dependency()` is invoked
- State is set to `ReservationState::Committed`
- `end = start` (since `is_filetransfer == true` and capacity is 0)
- No virtual reservation chain is created

---

### TC-3.3: Gateway RouterId Fallback — No Explicit gatewayRouterId Configured

**Objective:** Verify that when a `GatewayConfigDto` does not specify `gatewayRouterId`, the system falls back to generating `"AcI-Gateway-{component_id}"`.

**Given:**
- A `GatewayConfigDto` for `rms_0` with `gatewayRouterId` absent/empty:
  ```json
  { "rms_0": { "ingressBandwidthGbps": 1000, "egressBandwidthGbps": 1000, "gatewaySwitchId": "s0" } }
  ```
- Component ID is `rms_0`

**When:**
- The gateway configuration is parsed and the gateway RouterId is resolved

**Then:**
- The effective gateway RouterId is `RouterId::new("AcI-Gateway-rms_0")`
- This RouterId is used in link endpoint assignment for cross-RMS dependencies

---

### TC-3.4: Minimum and Maximum Bandwidth Capacities

**Objective:** Verify boundary handling for inter-gateway links with minimum/maximum bandwidth values.

**Given:**
- VRM with `rms_0` and `rms_1`
- An `InterGatewayLinkDto` with `bandwidthGbps = 1` (minimum) or `bandwidthGbps = i64::MAX`
- A data dependency requiring `bandwidth > link capacity`

**When:**
- The cross-RMS link reservation is scheduled at the ADC level

**Then:**
- For `bandwidthGbps = 1`: A single unit of bandwidth is modeled; scheduling succeeds only if the dependency's bandwidth requirement ≤ 1
- For `bandwidthGbps = i64::MAX`: Scheduling succeeds for any requested bandwidth within reason
- If the requested bandwidth exceeds the inter-gateway link capacity, the reservation is rejected and rollback occurs

---

### TC-3.5: Config Toggle — `USE_FULL_INTER_GATEWAY_PATH_FINDING = true`

**Objective:** Verify that when `USE_FULL_INTER_GATEWAY_PATH_FINDING` is `true`, the global `NetworkTopology` includes intermediate routers between gateways and uses k-shortest-paths.

**Given:**
- `USE_FULL_INTER_GATEWAY_PATH_FINDING = true`
- The global `NetworkTopology` includes intermediate switches between `AcI-Gateway-rms_0` and `AcI-Gateway-rms_1`
- A cross-RMS data dependency

**When:**
- `schedule_real_dependency()` processes the dependency
- The ADC-level `ResourceStore` performs path-finding

**Then:**
- Multiple path candidates are evaluated using k-shortest-paths
- The path with the best EFT is selected
- Intermediate router hop reservations are created (more than 4 total segments)
- The `NetworkTopology`'s k-shortest-paths cache is queried

---

### TC-3.6: Config Toggle — `USE_FULL_INTER_GATEWAY_PATH_FINDING = false` (Default)

**Objective:** Verify the default simple single-hop mode creates a direct virtual link.

**Given:**
- `USE_FULL_INTER_GATEWAY_PATH_FINDING = false` (default)
- A cross-RMS data dependency

**When:**
- The inter-gateway segment is scheduled

**Then:**
- A single direct virtual `LinkResource` is used between gateway RouterIds
- Capacity = `min(ingress_bandwidth_gbps, egress_bandwidth_gbps)` from source and target gateway configs
- No intermediate router nodes are considered for the inter-gateway hop
- Exactly 4 total link segments per dependency (2 internal + 2 virtual)

---

### TC-3.7: Workflow with Only Sync Dependencies, No Data Dependencies

**Objective:** Verify that a workflow with only sync dependencies (no data dependencies) across RMS boundaries still creates correct virtual reservation chains for the sync links.

**Given:**
- VRM with `rms_0` and `rms_1`
- `Workflow` with `task_A` on `rms_0`, `task_B` on `rms_1`, connected by `SyncDependency A ↔ B`
- No data dependencies

**When:**
- The workflow is scheduled

**Then:**
- `schedule_sync_dependencies()` processes the sync link
- `is_filetransfer = false` — the sync link is non-moldable
- A 4-segment virtual reservation chain is created for the sync dependency
- All segments reach `ReservationState::ReserveAnswer`
- The sync link's `reserved_capacity` equals `dependency.bandwidth`

---

## Category 4: Failure & Rollback Scenarios

### TC-4.1: Cross-RMS Dependency Fails on Source Internal Segment → Full Rollback

**Objective:** Verify that when the first segment (source_node → source_gateway) fails to schedule, NO segments are left in the store and all reservations are rolled back.

**Given:**
- VRM with `rms_0` and `rms_1`
- A cross-RMS data dependency where `rms_0`'s internal resources are fully saturated

**When:**
- `schedule_real_dependency()` attempts to schedule `source_node → AcI-Gateway-rms_0` on `rms_0`
- `rms_0`'s AcI returns `ReservationState::Rejected` for the probe/reserve of the internal segment

**Then:**
- The scheduler immediately returns `false`
- `cancel_all_reservations()` is invoked by the caller (`schedule_data_dependencies`)
- All previously scheduled reservations in `grid_component_res_database` are deleted
- No reservation segments for this dependency exist in `ReservationStore`
- `grid_component_res_database` is cleared
- The workflow's state is set to `ReservationState::Rejected`

---

### TC-4.2: Cross-RMS Dependency Fails on Second Virtual Segment (source_gateway → ADC) → Full Rollback

**Objective:** Verify that when a mid-chain virtual segment fails, all previously scheduled segments (including the successful first segment) are rolled back.

**Given:**
- VRM with `rms_0` and `rms_1`
- A cross-RMS data dependency
- Segment 1 (`source_node → source_gateway`) schedules successfully
- Segment 2 (`source_gateway → ADC`) fails because inter-gateway bandwidth is exhausted

**When:**
- The scheduler attempts the second virtual segment and receives `Rejected`

**Then:**
- The first segment's reservation is cancelled via `cancel_all_reservations()`
- Both the first (internal) and any tentatively created second segment reservations are removed
- `grid_component_res_database` is cleared
- `workflow.state == ReservationState::Rejected`
- No orphaned virtual reservations remain in `ReservationStore`

---

### TC-4.3: Cross-RMS Dependency Fails on Third or Fourth Segment → Full Rollback

**Objective:** Verify rollback when the target-side segments (3: `ADC → target_gateway`, or 4: `target_gateway → target_node`) fail.

**Given:**
- VRM with `rms_0` and `rms_1`
- A cross-RMS data dependency
- Segments 1 and 2 schedule successfully
- Segment 3 or 4 fails because `rms_1`'s resources are exhausted

**When:**
- The third or fourth segment returns `Rejected`

**Then:**
- All previously scheduled segments (1, 2, possibly 3) are rolled back
- `cancel_all_reservations()` successfully deletes all associated reservations across all components
- `workflow.state == ReservationState::Rejected`
- Invariant: Atomicity is preserved — either all 4 segments succeed or none do

---

### TC-4.4: Workflow Deadline Exceeded During Scheduling

**Objective:** Verify that if any node's computed start time + duration exceeds `workflow_booking_interval_end`, the entire workflow is rejected.

**Given:**
- VRM with `rms_0`
- A `Workflow` with `booking_interval_end = 500`
- A `NodeReservation` with `task_duration = 800` and a data dependency causing `start = 100` (so `100 + 800 = 900 > 500`)

**When:**
- `reserve()` computes `start + task_duration > workflow_booking_interval_end`

**Then:**
- `cancel_all_reservations()` is called immediately
- `workflow.state == ReservationState::Rejected`
- `reserve()` returns `false`

---

### TC-4.5: Co-Allocation Member Fails → All Group Members Rolled Back

**Objective:** Verify that if one member of a co-allocation group fails to schedule, previously scheduled group members are rolled back.

**Given:**
- VRM with `rms_0` and `rms_1`
- `CoAllocation(A, B)` where `A` is on `rms_0` and `B` is on `rms_1`
- `A` schedules successfully on `rms_0`
- `B` fails on `rms_1`

**When:**
- `schedule_co_allocation_node_reservations()` processes `B` and receives `Rejected`

**Then:**
- `schedule_co_allocation_node_reservations()` returns `false`
- The caller invokes `cancel_all_reservations()`
- `A`'s reservation is deleted from `rms_0`
- `workflow.state == ReservationState::Rejected`

---

### TC-4.6: Cancel All Reservations Handles Virtual Reservations

**Objective:** Verify that `cancel_all_reservations()` correctly deletes virtual reservations across multiple components, not just the entries in `grid_component_res_database`.

**Given:**
- A partially scheduled cross-RMS dependency with virtual reservations created in the store
- `grid_component_res_database` contains entries for some segments
- Virtual reservations exist in `store.original_to_virtual` for the original link reservation

**When:**
- `cancel_all_reservations(adc, &mut grid_component_res_database)` is called

**Then:**
- All entries in `grid_component_res_database` are deleted from their respective components
- Virtual reservations are cascade-deleted from `ReservationStore`
- `grid_component_res_database` is cleared
- No reservations from this workflow remain in any component

---

## Category 5: Lifecycle & Cleanup

### TC-5.1: Virtual Reservation Cascade-Delete on Parent Reservation Deletion

**Objective:** Verify that when a parent link reservation is deleted from `ReservationStore`, all associated virtual reservations are automatically cascade-deleted.

**Given:**
- A successfully scheduled cross-RMS data dependency with 2 virtual reservations tracked in `store.original_to_virtual`
- `original_to_virtual: { original_link_res_id → [virtual_id_1, virtual_id_2] }`

**When:**
- The original link reservation is deleted via `store.remove(original_link_res_id)`
- The cascade-delete logic executes

**Then:**
- `virtual_id_1` is removed from `ReservationStore`
- `virtual_id_2` is removed from `ReservationStore`
- The entry for `original_link_res_id` is removed from `store.original_to_virtual`
- Calling `store.get(virtual_id_1)` returns `None`
- Calling `store.get(virtual_id_2)` returns `None`

---

### TC-5.2: Workflow Rejection Sets All Child Reservations to Rejected

**Objective:** Verify the invariant: "If a workflow cannot be scheduled, all of its associated reservations are set to `Rejected`."

**Given:**
- A `Workflow` with 5 child `NodeReservation`s and 3 child `LinkReservation`s
- Scheduling fails partway through

**When:**
- `reserve()` returns `false` and sets `workflow.state = ReservationState::Rejected`

**Then:**
- All 8 child reservations in the workflow have state `ReservationState::Rejected` (or `ReservationState::Open`, depending on scheduling phase — but all non-scheduled are set appropriately)
- No child reservation remains in `ReservationState::ReserveAnswer` (which would indicate a partial commit)
- The invariant is satisfied: the workflow and its children are in a consistent terminal state

---

### TC-5.3: Probe Request Required Before Reservation (Pre-Flight Check)

**Objective:** Verify invariant 3: "Workflow reservations may only be reserved after a probe request has been executed for all workflow reservations and has completed successfully."

**Given:**
- A `Workflow` with multiple child reservations
- No probe has been executed

**When:**
- A reserve request is attempted without a prior successful probe

**Then:**
- The system rejects the reservation or enforces the probe-first rule
- `can_handel()` must be called (which checks feasibility) before `reserve()`

---

### TC-5.4: Removal of `RMS_GATEWAY_NAME` Constant — No References Remain

**Objective:** Verify that the global `RMS_GATEWAY_NAME` constant is fully removed and all code compiles using per-RMS gateway configuration.

**Given:**
- The source file `src/vrm/common/config.rs`

**When:**
- A grep search is performed for `RMS_GATEWAY_NAME`

**Then:**
- Zero references to `RMS_GATEWAY_NAME` exist anywhere in the codebase (except in this spec)
- `get_nodes_and_links()` in `src/vrm/rms/common.rs` accepts a per-RMS gateway name parameter
- All example JSON files use the new `gatewayConfig` section

---

### TC-5.5: Removal of `get_component_router_list()` — No References Remain

**Objective:** Verify that `get_component_router_list()` is fully removed per AD-5 (Information Hiding).

**Given:**
- The source file `src/vrm/vrm_component/vrm_component_manager/core.rs`

**When:**
- A grep search is performed for `get_component_router_list`

**Then:**
- Zero references to `get_component_router_list` exist anywhere in the codebase
- `HEFTSyncWorkflowScheduler::schedule_real_dependency()` no longer calls it
- The ADC only uses gateway RouterIds for routing decisions

---

## Category 6: Component Interaction & Integration

### TC-6.1: ADC-Level `ResourceStore` with Global `NetworkTopology`

**Objective:** Verify that the ADC-Master's `ResourceStore` contains a unified `NetworkTopology` aggregating all per-RMS topologies plus inter-gateway links.

**Given:**
- Two RMS components (`rms_0`, `rms_1`) each with internal topologies (switches, nodes, links)
- Inter-gateway links configured in `interGatewayLinks`
- `USE_FULL_INTER_GATEWAY_PATH_FINDING = true`

**When:**
- The global `NetworkTopology` is built at ADC initialization

**Then:**
- The topology includes all internal nodes/links from `rms_0`
- The topology includes all internal nodes/links from `rms_1`
- The topology includes `AcI-Gateway-rms_0` as a node
- The topology includes `AcI-Gateway-rms_1` as a node
- The topology includes inter-gateway links connecting the two gateways
- The `ResourceStore` can perform path-finding between `rms_0::node_1` and `rms_1::node_2` via the global topology

---

### TC-6.2: Gateway Nodes as Routing-Only Resources (Capacity = 0)

**Objective:** Verify that gateway nodes are stored in the ADC-level `ResourceStore` as `NodeResource`s with capacity = 0, indicating they are routing-only and cannot host compute tasks.

**Given:**
- ADC-level `ResourceStore` with gateway nodes registered

**When:**
- A `NodeReservation` attempts to probe the gateway node as a computation target
- A `LinkReservation` uses the gateway as an endpoint

**Then:**
- The gateway node's capacity is 0
- Probing the gateway for compute tasks returns `ProbeReservations::empty()` or signals infeasibility
- Link reservations using the gateway as an endpoint succeed (it functions as a routing hop)

---

### TC-6.3: Gateway Bandwidth Limits Enforced by `SlottedScheduleContext`

**Objective:** Verify that ingress/egress bandwidth limits from `GatewayConfigDto` are modeled and enforced in the network schedule.

**Given:**
- Gateway config for `rms_0`: `ingressBandwidthGbps = 500`, `egressBandwidthGbps = 500`
- Inter-gateway link: `bandwidthGbps = 10000`
- A data dependency requesting 600 Gbps of bandwidth from `rms_0 → rms_1`

**When:**
- The cross-RMS link segment `source_node → AcI-Gateway-rms_0` is scheduled internally on `rms_0`

**Then:**
- The internal segment on `rms_0` fails if the requested bandwidth (600 Gbps) exceeds the egress limit (500 Gbps)
- The inter-gateway segment succeeds (10000 > 600)
- The dependency fails at the egress-limited segment, triggering rollback

---

### TC-6.4: JSON Configuration Parsing — `GatewayConfigDto` and `InterGatewayLinkDto`

**Objective:** Verify that the new JSON configuration schema (`gatewayConfig` + `interGatewayLinks`) is correctly parsed from JSON.

**Given:**
- A JSON configuration file with:
  ```json
  {
    "gatewayConfig": {
      "rms_0": { "gatewayRouterId": "AcI-Gateway-rms_0", "ingressBandwidthGbps": 1000, "egressBandwidthGbps": 1000, "gatewaySwitchId": "s0" },
      "rms_1": { "gatewayRouterId": "AcI-Gateway-rms_1", "ingressBandwidthGbps": 2000, "egressBandwidthGbps": 2000, "gatewaySwitchId": "s1" }
    },
    "interGatewayLinks": [
      { "sourceGateway": "AcI-Gateway-rms_0", "targetGateway": "AcI-Gateway-rms_1", "bandwidthGbps": 10000 }
    ]
  }
  ```

**When:**
- The configuration is loaded and parsed via `serde_json`

**Then:**
- `GatewayConfigDto` for `rms_0` has `gateway_router_id = "AcI-Gateway-rms_0"`, `ingress_bandwidth_gbps = 1000`, `egress_bandwidth_gbps = 1000`
- `GatewayConfigDto` for `rms_1` has `gateway_router_id = "AcI-Gateway-rms_1"`, `ingress_bandwidth_gbps = 2000`, `egress_bandwidth_gbps = 2000`
- `InterGatewayLinkDto` has `source_gateway = "AcI-Gateway-rms_0"`, `target_gateway = "AcI-Gateway-rms_1"`, `bandwidth_gbps = 10000`
- No parsing errors

---

### TC-6.5: Backward Compatibility — Existing `TopologyDto` Fields Still Work

**Objective:** Verify that existing `TopologyDto` fields (`gatewaySwitchId`, `ingressBandwidthGbps`, `egressBandwidthGbps`) remain functional when the new `gatewayConfig` section is absent.

**Given:**
- A legacy JSON configuration without `gatewayConfig` or `interGatewayLinks` sections
- Only the existing `TopologyDto` fields are present

**When:**
- The system initializes with this configuration

**Then:**
- Gateway RouterId falls back to `format!("AcI-Gateway-{}", component_id)`
- `ingress_bandwidth_gbps` and `egress_bandwidth_gbps` are read from `TopologyDto`
- The system operates correctly (though with no cross-RMS capability without `interGatewayLinks`)

---

### TC-6.6: Information Hiding — ADC Does Not Enumerate Internal Routers

**Objective:** Verify that after removing `get_component_router_list()`, the ADC has no knowledge of internal RMS router topology beyond the gateway RouterId.

**Given:**
- An ADC with connected AcI components (each wrapping an RMS)

**When:**
- The ADC attempts to discover internal routers of child components

**Then:**
- No API exists to enumerate internal routers
- The only externally visible identifier is the per-RMS gateway RouterId
- All internal routing decisions are delegated to the respective AcI

---

### TC-6.7: `original_to_virtual` Tracking Map Integrity

**Objective:** Verify that the `original_to_virtual: HashMap<ReservationId, Vec<ReservationId>>` tracking map correctly persists across scheduling operations.

**Given:**
- A cross-RMS data dependency creates 2 virtual reservations
- The map is populated: `{ original_id → [virtual_1, virtual_2] }`

**When:**
- `remove_virtual_reservation(original_id, virtual_1)` is called
- Then `remove_virtual_reservation(original_id, virtual_2)` is called

**Then:**
- After the first removal: map still has entry `{ original_id → [virtual_2] }`
- After the second removal: the entire entry is removed (empty vec is cleaned up)
- The map is empty for this original reservation

---

## Category 7: End-to-End Integration Tests

### TC-7.1: Full Cross-RMS Workflow End-to-End

**Objective:** End-to-end test simulating a complete cross-RMS workflow from JSON parsing through scheduling to completion.

**Given:**
- A complete VRM system configuration JSON with:
  - 2 RMS systems with full internal topologies
  - Gateway config with per-RMS gatewayRouterIds
  - Inter-gateway links
  - A client workflow JSON with 4 tasks spanning both RMS systems, connected by data and sync dependencies
- Global clock initialized
- Reservation store empty

**When:**
- The system model is generated via `generate_system_model(file_path, store)`
- The workflow is submitted to the ADC-Master
- `HEFTSyncWorkflowScheduler::reserve()` is invoked

**Then:**
- All 4 node reservations reach `ReservationState::ReserveAnswer`
- All data dependencies are split into correct 4-segment chains
- All sync dependencies are split into correct 4-segment chains
- All virtual reservations are properly parent-linked
- No orphaned reservations remain
- `workflow.state == ReservationState::ReserveAnswer`
- The `VrmComponentManager` correctly tracks which component handles which reservation
- All invariants (1-5) are satisfied
