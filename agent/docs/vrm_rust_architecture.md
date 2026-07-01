# The VRM-Rust Implementation

This document outlines the current state of the VRM-Rust implementation, including all core components, data structures, and concepts. It specifies architectural principles and objectives that the current VRM-Rust implementation includes, and provides a detailed architectural view of the hierarchical VRM-Rust system. Thereafter, it continues to explain all the core components, data structures, and concepts of the VRM-Rust system.

*Note: Specific VRM-Rust components are referenced using code formatting (e.g., `traits` and `structs`).*

## 1. Features and Capabilities

This section outlines the functional features and capabilities of the VRM-Rust system. It further specifies divergences between the implementation and the theoretical VRM concept.

* **Abstraction and Usability:** The implementation achieves the objective of providing users with an abstraction layer for virtual resources and SLAs without requiring knowledge of the underlying infrastructure.
* **Robustness and Fault Tolerance:** This objective is partially realised through frequent monitoring of local RMS resources. These mechanisms synchronise the current local RMS state with the meta-scheduler to cancel tasks or workflows when a reservation is jeopardised. The specified failure recovery mechanisms proposed by Burchard et al. for the VRM are not implemented.
* **Diversification and Heterogeneity:** The concept architecture of the VRM supports the integration of planning- and queuing-based RMSs with varying levels of autonomy. The VRM-Rust implementation includes only a Slurm RMS adapter that connects to the RMS via the Slurm REST API and operates at the fourth level of autonomy, *Priority for Local Requests*.
* **Security:** The concept of information hiding, which ensures that lower layer resources and topologies remain abstracted from higher layers, is realised in the VRM-Rust system. The `ADC` incorporates this concept by aggregating all virtualised resources of the underlying system and only transmits these aggregated resources without location information to higher levels. Furthermore, data regarding node occupation and task queues from the underlying local RMS is processed within the connected `AcI` and is not distributed to other `VrmComponent`s.
* **Guaranteeing Negotiated SLAs:** The VRM concept specifies that support and enforcement for SLAs are provided. The VRM-Rust system is capable of providing the Advance Reservation and deadline guarantees as SLAs.
* **System Simulation:** The system enables simulations that emulate configured VRM environments with an arbitrary number of `AcI` and `ADC` instances. It is possible to emulate cluster nodes, cluster network topologies, or both simultaneously.

## 2. VRM-Rust Overview

This section details the VRM-Rust system architecture through the life cycle of a client reservation request for an atomic task or a complex workflow.

The system architecture allows for atomic task or workflow submission from `Client`s, which are registered within the system by unique identifiers. Upon submission, resource requests are preprocessed into a structured format to enable efficient scheduling. The `VrmManager` orchestrates this process and transmits unprocessed workflows or atomic tasks to the *Master* `ADC`, which serves as the entry point of the system. The *Master* `ADC` distinguishes between a workflow and an atomic task. The system forwards atomic tasks directly to the `VrmComponentManager` of the *Master* `ADC`. Workflows are instead directed to the `WorkflowScheduler` for a feasibility analysis. This process determines whether the system can handle all tasks within the workflow before the entire request is submitted via the `VrmComponentManager`.

The `VrmComponentManager` then submits the tasks to the underlying `VrmComponent`s, which consist of `AcI`s and/or `ADC`s. These components distribute the requests to their connected subsystems. The `ADC` tracks reservations on underlying components and aggregates performance data and results from requested operations.

The `AcI` features an `AdvanceReservationRms` adapter that links the RMS of the HPC cluster to the VRM system. For Slurm-based RMSs, the `SlurmRms` adapter connects the physical RMS system to the VRM system through the Slurm REST API, facilitating task and node synchronisation as well as task submission. Additionally, three simulation adapter mocks are implemented: `RmsNetworkSimulator`, `RmsNodeSimulator`, and `RmsSimulator`. Furthermore, the `AdvanceReservationRms` interface provides the functionality of *shadow scheduling*. This capability allows for *what-if* planning phases or schedule optimisations in a sandbox environment, without executing actions on the official schedule.

In instances where the underlying *RMS* employs a queuing-based system rather than a planning-based one (such as Slurm), the adapter reflects the current reservation state of the physical RMS in the `Schedule` (which contains the current state of the RMS system and the requested Advance Reservation for a specific RMS). The `Schedule` implementation uses a generic **strategy pattern** via `SlottedScheduleContext<S: SlottedScheduleStrategy>`, where the strategy type parameter `S` is resolved at compile time. Two concrete strategies exist: `NodeStrategy` for compute node capacity tracking, and `LinkStrategy` for network bandwidth management across paths. The `LinkStrategy` incorporates the `NetworkTopology`, which contains the underlying link infrastructure and a K-shortest-paths cache to facilitate path routing within the network.

### 2.1 Workflow

`Workflow`s are submitted to the VRM in JSON format, containing a unique *ClientId*. The system transforms the raw JSON into multiple Data Transfer Objects (DTOs) to decouple the internal system implementation from the API. These DTOs facilitate the construction of a directed workflow graph, where each `WorkflowNode` represents a task that contains incoming and outgoing dependencies, a co-allocation group id and the `ReservationBase`, which contains relevant metadata for the Advance Reservation mechanism. The edges within this graph are represented by two distinct dependency types:

* **Data Dependencies (`DataDependency`):** These represent asynchronous, file-based data transfers. They are the required input for task execution or the output generated by task completion.
* **Synchronous Dependencies (`SyncDependency`):** These dependencies represent real-time communication of tasks during their execution.

Based on the synchronous dependencies, a co-allocation graph is constructed, where each co-allocation consists of either an atomic task or a group of tasks that are treated as a single atomic task by the `WorkflowScheduler`.

### 2.2 WorkflowScheduler

The *Master* `ADC` serves as the entry point for the VRM, utilising the `WorkflowScheduler` to preprocess all incoming `WorkflowReservation`s. To reduce scheduling complexity, resource fragmentation, and deadlocks, the scheduler is attempting to reserve all requested nodes and links atomically within a `WorkflowReservation`. If the full set of resources cannot be secured, the entire workflow request is rejected. This approach ensures that the system state remains consistent and prevents the overhead associated with partial allocation cleanup.

The default workflow scheduling algorithm is the `HEFTSyncWorkflowScheduler`, which implements the **Heterogeneous Earliest Finish Time with Synchronization** (HEFT) algorithm. It uses a two-phase approach: an upward rank calculation to prioritise tasks by critical path, followed by an earliest-finish-time-based processor selection across components. Furthermore, the scheduler manages the synchronisation of dependencies within co-allocation groups by ensuring that all tasks are assigned synchronised start times and remain connected via the specified links. This architectural design ensures that downstream `VrmComponent`s operate on atomic reservations rather than complex, unparsed workflow structures, thereby enhancing the reliability and determinism of the scheduling logic.

### 2.3 VrmComponent

The VRM architecture is composed of fundamental building blocks, the `VrmComponent`s. These components are organised into a hierarchical tree structure consisting of `ADC`s and `AcI`s. Within this hierarchy, `ADC`s serve as internal nodes, whereas the `AcI`s are the leaf nodes. The root `ADC` is defined as the master `ADC` and serves as the entry point for the system. The smallest valid VRM system is a single master `ADC` as a root with one `AcI` as a child. All `AcI`s and `ADC`s run in their own thread.

The `ADC` functions as an abstraction layer for the requester, providing a unified interface that encapsulates the complexity of the underlying infrastructure. It owns a `VrmComponentManager` that is organised into five sub-modules: **core** (CRUD operations on child components), **scheduling** (probe/reserve/delete orchestration), **metrics** (satisfaction and load aggregation), **shadow** (shadow schedule lifecycle management), and **tracking** (reservation-to-component mapping). The manager is responsible for the downward distribution of requests to subordinate components and the upward aggregation of their respective responses.

The `AcI` structure serves as a special connector for HPC environments. It enables the management and execution of atomic tasks and communication with a physical RMS.

The ADC follows the **information hiding principle**: it only knows each child component's **gateway RouterId** (derived as `"AcI-Gateway-{component_id}"`), not the internal router topology. The `get_component_router_list()` method has been removed; internal routing within an RMS is the AcI's responsibility. The `get_component_gateway_router_id()` method provides the only externally visible router identifier for child components.

Both the `AcI` and `ADC` are implemented following the **actor pattern**. Each component operates within its own dedicated thread, spawned by the `RegistryClient` with a descriptive name (e.g., `"Actor-MyAcI"`). Communication between components occurs exclusively via `mpsc` channels through the `VrmComponentProxy`, which serialises `VrmComponent` trait method calls into `VrmMessage` variants. The proxy uses a synchronous request-reply pattern: it sends a message containing a `oneshot` reply channel and blocks the caller until the actor responds. This design enables concurrency — if a component is blocked while waiting to receive requested information from an RMS, other parts of the system can still continue to function. However, the synchronous nature of proxy calls means that mutual cross-component calls can lead to deadlock if not carefully managed.

### 2.4 Resource Management System (RMS)

The `RMS` serves as the abstraction layer, connecting the `AcI` with the schedule and the HPC environment. The RMS component follows a **trait-based strategy pattern** organised in three layers:

1. **`Rms` trait:** The fundamental contract that all RMS adapters implement, providing `commit()`, `delete_task()`, and `get_active_schedule()`.
2. **`AdvanceReservationRms` trait:** Extends `Rms` with the full reservation management life cycle (`probe()`, `reserve()`, `probe_best()`, shadow schedule operations, fragmentation and load metrics, and capability checks).
3. **Concrete implementations:** Four adapters — `SlurmRms` (full node+network, live Slurm REST API with background sync loop), `RmsSimulator` (full node+network simulation), `RmsNodeSimulator` (node-only), and `RmsNetworkSimulator` (link-only).

The `RmsNodeNetwork` marker trait provides a **blanket implementation** of `AdvanceReservationRms` for any type implementing both `Rms` and the `Helper` trait, eliminating code duplication between node+network-capable RMS variants. The factory pattern via `RmsSystemWrapper::get_instance()` decouples configuration deserialisation from instantiation, allowing runtime selection of the appropriate RMS adapter.

In simulation environments, the system utilises one of the simulation modules. For actual Slurm deployments, the `SlurmRms` handles communication via the `SlurmRestApiClient` (a `reqwest`-based REST client) and spawns a background tokio task that periodically polls node and job state to synchronise the local `ResourceStore` and `ReservationStore` with the physical cluster. A `BiMap<ReservationId, u32>` translates between VRM reservation IDs and Slurm job IDs during commit and delete operations.

### 2.5 Schedule

The `Schedule` maintains the state of both active and pending tasks of the connected RMS. Furthermore, `Schedule` is used to enable queue-based architectures, such as Slurm, to support planning-based reservations.

The schedule implementation follows a **generic strategy pattern** with compile-time polymorphism. The core data structure is `SlottedScheduleContext<S: SlottedScheduleStrategy>`, which manages a circular buffer of time-division `Slot`s. Each `Slot` tracks physical capacity, currently reserved load, and associated reservation IDs. A **sliding window** design advances the scheduling window as simulation time progresses, moving expired historical data to a `LoadBuffer` for long-term metrics.

Two concrete strategies implement the `SlottedScheduleStrategy` trait:
* **`NodeStrategy`:** Manages single-resource capacity (e.g., CPU cores on one compute node). Fragmentation is calculated per-slot based on load vs. capacity.
* **`LinkStrategy`:** Manages network bandwidth across a grid topology. Uses K-shortest paths from the `NetworkTopology` to route reservations across multiple links.

A distinction is made between the Master Schedule and Shadow Schedules:
* **Master Schedule:** Acts as the official real-time mirror of the underlying RMS.
* **Shadow Schedules:** Utilised for *what-if* planning phases. These schedules allow complex optimisation and simulation without impacting the live environment.

This dual-schedule approach enables the system to perform isolated optimisations. If the outcome of a simulation within a Shadow Schedule meets the required performance criteria, that schedule can be promoted to replace the Master Schedule, ensuring a safe transition. Two fragmentation algorithms are available for measuring schedule quality: the **Quadratic Mean** method (O(n), cached) and the more expensive **Resubmission** method (simulates releasing and re-booking reservations).

### 2.6 Stores

The VRM system deploys object stores to handle interactions with resources and reservations. These stores oversee all operations related to their respective objects, controlling access and mutations via unique keys. The architecture defines a system-wide `ReservationStore` and a separate `ResourceStore` for each individual RMS.

* **`ReservationStore`:** This store is the **central thread-safe repository** for all reservations, using `Arc<RwLock<StoreInner>>` with `parking_lot` primitives. Primary storage is via a `SlotMap<ReservationId, Arc<RwLock<Reservation>>>` with secondary indexes by name, client, and handler. It supports **snapshot isolation** via the `snapshot()` method, which creates a deep-cloned copy for schedulers to work on without lock contention. It also manages **virtual (shadow) reservations** for link path exploration. The `original_to_virtual: HashMap<ReservationId, Vec<ReservationId>>` tracking map links original link reservations to their derived virtual reservations, enabling cascade-delete when the parent is removed. All reservation types implement the `ReservationTrait` trait, which provides a uniform interface over the three concrete types. The store includes a notification service that allows `VrmComponent`s to register as `ReservationNotificationListener`s, ensuring registered components are automatically notified of state transitions.
* **`ResourceStore`:** This store contains all physical link and node resources belonging to the underlying RMS infrastructure. Nodes and links are stored within separate `SlotMap`-indexed structures. It also maintains a K-shortest paths cache for network topology queries and a router list for access point management. Resources implement the `Resource` trait, which provides a polymorphic interface via `as_any()` downcasting and `can_handle_request()` feasibility checks against a `FeasibilityRequest` enum (discriminating between `Node` and `Link` requests). The store provides two admission control entry points: `can_handle_adc_request()` (by value) and `can_handle_aci_request()` (by reference).

### 2.7 Reservation State Notification System

The `ReservationStore` implements an observer pattern via the `ReservationNotificationListener` trait, allowing `VrmComponent`s to subscribe to state changes for a reservation. The `VrmStateListener` implements this trait and maintains a set of open reservation IDs, automatically removing terminal-state reservations (`Deleted`, `Rejected`, `Finished`). This notification system is central for the functionality of the `VrmManager`.

The `VrmManager` utilises notifications to manage the life cycle of reservations for submitted workflows, links, or node resource requests. It manages state transitions from open to probe, probe to reserve, reserve to commit, and any state to delete. Upon receiving a notification that a reservation state has been updated, the `VrmManager` proceeds with the reservation according to its defined life cycle.

### 2.8 The Reservation Life Cycle

A reservation in the VRM system represents a resource request made by a `Client`. These reservations are derived from the workflow or atomic task submitted by the `Client`. There are three kinds of reservations: `NodeReservation`, `LinkReservation` and `WorkflowReservation` (contains all link- or node reservations for the corresponding workflow).

The life cycle of these reservations is defined by the five `ReservationProceeding`s that specify the requested action for each reservation made by the `Client`.

These reservation proceedings are the following:
* **Probe:** This request returns a `ProbeReservation` object that includes all feasible resource reservations capable of fulfilling the specified requirements. This request checks all connected RMS environments to the VRM system for feasible resources that match the requirement.
* **Reserve:** Temporarily reserve a resource with the specified requirements at the corresponding `Schedule` by first initialising a probe request to determine the best resource reservation in the VRM or reserving directly a feasible resource. These reservations do not affect the actual resources, they remain in the `Schedule` until the following *Commit* or *Delete* action is requested.
* **Commit:** Allocates a resource that matches the specified requirements by first initiating a reserve request with these specifications and then allocating these reserved resources at the corresponding physical RMS system.
* **Delete:** Deletes a specified reserved or allocated resource.
* **Ignore:** The VRM-Rust system will not interact with this reservation, as it has no authority over it (reservation was submitted via a local RMS).

To guarantee the system consistency, the following invariants are maintained over the reservation life cycle:
* **Atomic Promotion:** A successful `ReserveProbeReservation` must atomically invalidate all other `ProbeReservation`s and replace the associated parent *ProbeAnswer*.
* **Terminal Immutability:** For the states `{Finished, Rejected, Deleted}`, no further transitions are defined.
* **Cleanup:** Any transition into a terminal state releases/cleans up the reserved/allocated resources.

#### 2.8.1 Probe Reservation Process
The probe reservation process within the VRM architecture is distinct from others because it requires multiple state changes to succeed. This process is instantiated by the `VrmManager`, which updates the reservation state from *Open* to *ProbeAnswer*. This update indicates that a probe request for this reservation has been made. The potential outcomes of this operation are *Rejected* upon failure or *ReserveAnswer* following a successful reserve request.

During the probe process, the system queries all connected `AcI` components to return all valid *ProbeReservation* objects that satisfy the specific requirements of the reservation. These objects are aggregated into a `ProbeReservations` container. This container encapsulates the original probe reservation and all received probe reservations with their respective *AcIId* to ensure origin traceability. An important difference between a probe reservation and a normal reservation is that probe reservations are not tracked by the `ReservationStore`.

These aggregated `ProbeReservations` are returned to the requester to initiate the promotion process. The system selects the best candidate from the `ProbeReservations` object based on selection criteria, such as the earliest start time, using comparators such as `EFTReservationCompare`. The selected candidate replaces the original reservation, and the state is updated to `ReserveProbeReservation`. The reservation is directly via a reserve request submitted to the `AcI`, which issued the probe reservation. If the reserve request succeeds, the state is updated to `ReserveAnswer` and the probe reservation process terminates. In the event of a failure, the system discards the promoted candidate and selects the next best candidate for promotion.

#### Reservation States

| State (S) | Category | Description |
| :--- | :--- | :--- |
| **Open** | Active | Entry state for all new resource requests, which wait to be processed by the VRM. |
| **ProbeAnswer** | Active | Feasibility was successfully confirmed, and all feasible reservation options are returned. |
| **ProbeReservation** | ProbeAnswer | Specific candidates for a specific time slot and resource mapping. |
| **ReserveProbeReservation**| ProbeAnswer | Starts the promotion process from ProbeReservation to ProbeAnswer. |
| **ReserveAnswer** | Active | Resources are temporarily reserved for the client. |
| **Committed** | Active | Reserved resources are allocated, and task execution begins. |
| **Rejected** | Terminal | Request denied due to policy or resource constraints. |
| **Finished** | Terminal | Successful completion of the associated tasks and resources is released. |
| **Deleted** | Terminal | Explicit cancellation of the reservation by the client or VRM system. |
| **External** | Terminal | The reservation represents an externally submitted job from a local RMS, which the VRM-Rust system only tracks. |


### 2.9 Gateway Abstraction and Cross-RMS Routing

Each RMS cluster is abstracted behind a single **gateway node** with a unique, per-RMS `RouterId` (e.g., `"AcI-Gateway-rms_0"`). The gateway name is configurable via a `GatewayConfigDto` in the VRM JSON configuration. If not explicitly set, it falls back to `"AcI-Gateway-{component_id}"`.

Gateway nodes are stored in the RMS-level `ResourceStore` with `capacity = -1` (routing-only, cannot host compute tasks). Ingress and egress bandwidth limits from the `TopologyDto` are enforced via the gateway's link entries connecting to the internal switch topology.

#### Cross-RMS Data Dependencies (4-Segment Virtual Reservation Chain)

When a data dependency spans two different RMS components, the link is split into a chain of four virtual link reservations tracked in the `ReservationStore`:

```
source_node → source_gateway  (on source AcI, internal)
source_gateway → ADC-System   (virtual, ADC level)
ADC-System → target_gateway   (virtual, ADC level)
target_gateway → target_node  (on target AcI, internal)
```

All four segments are tracked as `LinkReservation`s with `parent_reservation_id` references via the `original_to_virtual` map. Virtual reservations are **only accessible by the original parent reservation** and are automatically cascade-deleted when the parent is removed, preserving the atomicity invariant: if any segment fails to schedule, all previously scheduled segments are rolled back via `cancel_all_reservations()`.

#### Configuration Toggle

A boolean configuration flag `USE_FULL_INTER_GATEWAY_PATH_FINDING` controls the inter-gateway routing strategy:
- **`false` (default):** Treat gateway-to-gateway as a direct virtual `LinkResource` with capacity = `min(ingress_bandwidth_gbps, egress_bandwidth_gbps)`.
- **`true`:** Use full multi-hop k-shortest-paths via the global `NetworkTopology` for inter-gateway routing (requires intermediate routers/switches between gateways to be configured).

#### Per-RMS Gateway Configuration

Gateway configuration is specified in the JSON configuration file under a dedicated `gatewayConfig` section, separate from the existing `TopologyDto` fields:

```json
{
  "gatewayConfig": {
    "rms_0": {
      "gatewayRouterId": "AcI-Gateway-rms_0",
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