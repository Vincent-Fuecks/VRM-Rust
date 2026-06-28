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
