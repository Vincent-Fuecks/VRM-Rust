# Workflow Architecture

## Overview

The Workflow component manages the representation and scheduling of complex multi-task workflows submitted to the VRM system. A workflow consists of a directed graph of `WorkflowNode`s connected by data and sync dependencies.

## Workflow Graph Structure

### Nodes
Each `WorkflowNode` represents a computational task with:
- A reference to its `NodeReservation` in the `ReservationStore`
- Lists of incoming/outgoing data and sync dependencies
- A co-allocation group key

### Dependencies
- **Data Dependencies:** Represent file-based data transfers between tasks
- **Sync Dependencies:** Represent real-time communication requirements during task execution

### Co-Allocation Groups
Tasks connected by sync dependencies form co-allocation groups. All members of a group must be scheduled simultaneously with matching start times.

## WorkflowScheduler

The `HEFTSyncWorkflowScheduler` implements the HEFT (Heterogeneous Earliest Finish Time) algorithm extended with synchronization support for co-allocated tasks.

### Scheduling Phases
1. **Prioritization:** Tasks are sorted by upward rank (critical path length)
2. **Processor Selection:** Each task is assigned to the component that minimizes its Earliest Finish Time (EFT)
3. **Dependency Resolution:** Data and sync dependencies are scheduled as link reservations

## Gateway Abstraction

Per AD-5 (Information Hiding), the ADC only knows each child component's gateway `RouterId`. The `get_component_router_list()` method has been removed. Internal routing within an RMS is delegated to the AcI.

### Same-RMS Dependencies
For same-RMS dependencies, link reservations use gateway router IDs as endpoints, and the AcI handles internal routing via its topology.

### Cross-RMS Dependencies (4-Segment Virtual Chain)
Cross-RMS data dependencies are split into a chain of four virtual link reservations:
1. `source_node → source_gateway` (on source AcI)
2. `source_gateway → ADC-System` (virtual, ADC level)
3. `ADC-System → target_gateway` (virtual, ADC level)
4. `target_gateway → target_node` (on target AcI)

All segments must succeed or all are rolled back via `cancel_all_reservations()`, preserving atomicity.
Virtual reservations are tracked in `ReservationStore.original_to_virtual` and cascade-deleted when the parent is removed.
