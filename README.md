# VRM-Rust

A hierarchical Virtual Resource Manager (VRM) implementation in Rust, designed to provide an abstraction layer for virtual resources and Service Level Agreements (SLAs) in High-Performance Computing (HPC) environments.

## Table of Contents

- [VRM-Rust](#vrm-rust)
  - [Table of Contents](#table-of-contents)
  - [Overview](#overview)
    - [Features and Capabilities](#features-and-capabilities)
    - [Reservation](#reservation)
      - [Probe Reservation Process](#probe-reservation-process)
      - [Reservation State Definitions](#reservation-state-definitions)
  - [Pre-Requirements](#pre-requirements)
    - [Installation](#installation)
    - [Usage Modes](#usage-modes)
      - [Option A: RmsNodeSimulator (Quick Start)](#option-a-rmsnodesimulator-quick-start)
      - [Option B: SlurmRms](#option-b-slurmrms)
        - [Step 1 Clone and Initialize Environment](#step-1-clone-and-initialize-environment)
        - [Step 2 Configure Authentication](#step-2-configure-authentication)
        - [Step 3 Generate Access Token](#step-3-generate-access-token)
        - [Step 4 Verify Connection (Optional)](#step-4-verify-connection-optional)
        - [Step 5: Verify Container Configuration (Optional)](#step-5-verify-container-configuration-optional)
        - [Step 6 Configure VRM-Rust](#step-6-configure-vrm-rust)
        - [Step 7 Run the VRM-Rust with Demo data](#step-7-run-the-vrm-rust-with-demo-data)
  - [Project Structure (Overview)](#project-structure-overview)
  - [VRM-Rust Prototype Documentation: Ideas, Unimplemented Features, and Optimizations](#vrm-rust-prototype-documentation-ideas-unimplemented-features-and-optimizations)
    - [Ideas](#ideas)
    - [Not Implemented](#not-implemented)
    - [Optimizations](#optimizations)

<details><summary>VRM-Rust Overview</summary>

## Overview

This section details the VRM-Rust system architecture through the life cycle of a client reservation request for an atomic task or a complex workflow. This architectural process is illustrated in Figure [Architecture Diagram](#arch-diagram). 

The system architecture allows for atomic task or workflow submission from **Client**s, which are registered within the system by unique identifiers. Upon submission, resource requests are preprocessed into a structured format to enable efficient scheduling. The **VrmManager** orchestrates this process and transmits unprocessed workflows or atomic tasks to the Master **ADC**, which serves as the entry point of the system. The Master **ADC** distinguishes between a workflow and an atomic task. The system forwards atomic tasks directly to the **VrmComponentManager** of the Master **ADC**. Workflows are instead directed to the **WorkflowScheduler** for a feasibility analysis. This process determines whether the system can handle all tasks within the workflow before the entire request is submitted via the **VrmComponentManager**. 

The **VrmComponentManager** then submits the tasks to the underlying **VrmComponent**s, which consist of **AcI**s and/or **ADC**s. These components distribute the requests to their connected subsystems. The **ADC** tracks reservations on underlying components and aggregates performance data and results from requested operations.

The **AcI** features an **AdvanceReservationRms** adapter that links the RMS of the HPC cluster to the VRM system. For Slurm-based RMSs, the **SlurmRms** adapter connects the physical RMS system to the VRM system through the Slurm REST API, facilitating task and node synchronisation as well as task submission. Additionally, three simulation adapter mocks are implemented: **RmsNetworkSimulator**, **RmsNodeSimulator**, and **RmsSimulator**. Furthermore, the **AdvanceReservationRms** interface provides the functionality of **shadow scheduling**. This capability allows for what-if planning phases or schedule optimisations in a sandbox environment, without executing actions on the official schedule. 

In instances where the underlying RMS employs a queuing-based system rather than a planning-based one (such as Slurm), the adapter reflects the current reservation state of the physical RMS in the **Schedule** (which contains the current state of the RMS system and the requested Advance Reservation for a specific RMS). The **Schedule** implementation uses a generic **strategy pattern** via `SlottedScheduleContext<S: SlottedScheduleStrategy>`, where the strategy type is resolved at compile time. Two concrete strategies exist: **NodeStrategy** for compute node capacity tracking, and **LinkStrategy** for network bandwidth management across paths. The latter incorporates the **NetworkTopology**, which contains the underlying link infrastructure and a K-shortest-paths cache to facilitate path routing within the network.

<a name="arch-diagram"></a>
![Architecture Diagram](./diagrams/architecture.svg)
*Figure 1: System Architecture*

### Features and Capabilities

- **Abstraction & Usability:** Provides a high-level interface for virtual resources and SLAs.
- **Slurm Support:** Integrates with Slurm-based RMSs via the Slurm REST API.
- **Security & Information Hiding:** Uses a hierarchical aggregation model (ADC) to hide underlying resource topologies from higher layers.
- **SLA Enforcement:** Guarantees Advance Reservations and execution deadlines.
- **System Simulation:** Built-in support for emulating cluster nodes and network topologies for testing and development.

### Reservation

A reservation in the VRM system represents a resource request made by a **Client**. These reservations are derived from the workflow or atomic task submitted by the **Client**. There are three kinds of reservations: **NodeReservation**, **LinkReservation** and **WorkflowReservation** (contains all link- or node reservations for the corresponding workflow). 

The life cycle of these reservations is defined by the five **ReservationProceeding**s that specify the requested action for each reservation made by the **Client**. 


These reservation proceedings are the following:

* Probe: This request returns a **ProbeReservation** object that includes all feasible resource reservations capable of fulfilling the specified requirements. This request checks all connected RMS environments to the VRM system for feasible resources that match the requirement. 
* Reserve: Temporarily reserve a resource with the specified requirements at the corresponding **Schedule** by first initialising a probe request to determine the best resource reservation in the VRM or reserving directly a feasible resource. These reservations do not affect the actual resources, they remain in the **Schedule** until the following Commit or Delete action is requested.
* Commit: Allocates a resource that matches the specified requirements by first initiating a reserve request with these specifications and then allocating these reserved resources at the corresponding physical RMS system. 
* Delete: Deletes a specified reserved or allocated resource.
* Ignore: The VRM-Rust system will not interact with this reservation, as it has no authority over it (reservation was submitted via a local RMS).

The reservation proceeding is tracked by the nine **ReservationState**s detailed in Table [Reservation State Definitions](#reservation-state-definitions). These states define the current stage in the reservation life cycle and specify the potential transitions to subsequent states, as illustrated in Figure Figure [Reservation Life Cycle](#reservation-diagram). To guarantee the system consistency, the following invariants are maintained over the reservation life cycle: 

* Atomic Promotion: A successful **ReserveProbeReservation** must atomically invalidate all other **ProbeReservation** and replace the associated parent ProbeAnswer. 
* Terminal Immutability: For the states $\{Finished, Rejected, Deleted\}$, no further transitions are defined.
* Any transition into a terminal state releases/cleans up the reserved/allocated resources.

#### Probe Reservation Process

The probe reservation process within the VRM architecture is distinct from others because it requires multiple state changes to succeed. This process is instantiated by the **VrmManager**, which updates the reservation state from Open to ProbeAnswer. This update indicates that a probe request for this reservation has been made. The potential outcomes of this operation are Rejected upon failure or ReserveAnswer following a successful reserve request.

During the probe process, the system queries all connected **AcI** components to return all valid ProbeReservation objects that satisfy the specific requirements of the reservation. These objects are aggregated into a **ProbeReservations** container. This container encapsulates the original probe reservation and all received probe reservations with their respective AcIId to ensure origin traceability. An important difference between a probe reservation and a normal reservation is that probe reservations are not tracked by the **ReservationStore**. 

These aggregated **ProbeReservations** are returned to the requester to initiate the promotion process. The system selects the best candidate from the **ProbeReservations** object based on selection criteria, such as the earliest start time. The selected candidate replaces the original reservation, and the state is updated to ReserveProbeReservation. The reservation is directly via a reserve request submitted to the **AcI**, which issued the probe reservation. If the reserve request succeeds, the state is updated to ReserveAnswer and the probe reservation process terminates. In the event of a failure, the system discards the promoted candidate and selects the next best candidate for promotion.

#### Reservation State Definitions

| State                       | Category    | Description                                                                                          |
| :-------------------------- | :---------- | :--------------------------------------------------------------------------------------------------- |
| **Open**                    | Active      | Entry state for all new resource requests, which wait to be processed by the VRM.                    |
| **ProbeAnswer**             | Active      | Feasibility was successfully confirmed, and all feasible reservation options are returned.           |
| **ProbeReservation**        | ProbeAnswer | Specific candidate for a specific time slot and resource mapping.                                    |
| **ReserveProbeReservation** | ProbeAnswer | Starts the promotion process from ProbeReservation to ProbeAnswer.                                   |
| **ReserveAnswer**           | Active      | Resources are temporarily reserved for the client.                                                   |
| **Committed**               | Active      | Reserved resources are allocated, and task execution begins.                                         |
| **Rejected**                | Terminal    | Request denied due to policy or resource constraints.                                                |
| **Finished**                | Terminal    | Successful completion of the associated tasks and resources is released.                             |
| **Deleted**                 | Terminal    | Explicit cancellation of the reservation by the client or VRM system.                                |
| **External**                | Terminal    | The reservation represents an externally submitted job from a local RMS, which the VRM-Rust system only tracks. |

<a name="reservation-state-definitions"></a>
*Table 1: Reservation State Definitions*

<a name="reservation-diagram"></a>
![Reservation Life Cycle](./diagrams/reservation_life_cycle.svg)
*Figure 2: Reservation Life Cycle*
</details>

## Pre-Requirements

Before you begin, ensure you have the following installed:

- **[Rust](https://rust-lang.org/)** 
- **[Docker](https://docs.docker.com/get-docker/)**
- **[Docker Compose](https://docs.docker.com/compose/install/)**

### Installation

Clone the VRM-Rust repository and navigate into the project directory:

```bash
# Clone the VRM-Rust Repository
git clone https://github.com/Vincent-Fuecks/VRM-Rust.git
```

### Usage Modes 

#### Option A: RmsNodeSimulator (Quick Start)

To test VRM-Rust against a Slurm-based system, you must first set up the virtual cluster.

```bash
cargo run -- --input-file data/workflow_with_direct_mapping.json --config-file data/vrm_node_simulator.json
```

#### Option B: SlurmRms 

To test VRM-Rust against a Slurm-based system, follow these steps to set up the virtual cluster.

##### Step 1 Clone and Initialize Environment

```bash
# Clone the Virtual Slurm Environment (modified clone form https://github.com/giovtorres/slurm-docker-cluster)
git clone https://github.com/Vincent-Fuecks/virtual-slurm-environment.git
cd virtual-slurm-environment

# Start the cluster to initialize directories
sudo docker compose up -d
```

##### Step 2 Configure Authentication

Inject the JWT key into the docker cluster and set the correct permissions:

```bash
# Generate a JWT key for authentication
openssl rand -out jwt_hs256.key 32

# Copy key to the control daemon
sudo docker cp jwt_hs256.key slurmctld:/etc/slurm/jwt_hs256.key

# Set ownership inside the volume
sudo docker exec -u root slurmdbd chown 990:990 /etc/slurm/jwt_hs256.key
sudo docker exec -u root slurmdbd chmod 600 /etc/slurm/jwt_hs256.key

# Update all compute nodes
sudo docker compose up -d
sudo ./update_slurmfiles.sh slurm.conf

# Open up /data permissions 
sudo docker exec -it slurmctld chmod 777 /data

# Add Slurm user to Slurm container and compute nodes (slurmctld slurmrestd c0 c1 c2 c3 c4 c5 c6)
sudo ./add_vrmUser.sh
```

##### Step 3 Generate Access Token

Generate a REST API token for the user **vrmUser**. We’ll set a lifespan of one day for the token:

```bash
sudo docker compose exec slurmctld scontrol token username=vrmUser lifespan=86400
```

> [!IMPORTANT]
>
> Copy the token output from the command above. You will need it for the final configuration.

##### Step 4 Verify Connection (Optional)

Test the Slurm REST API connection using curl:

```bash
curl -s -v \
  -H "X-SLURM-USER-NAME: vrmUser" \
  -H "X-SLURM-USER-TOKEN: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3Nzk4MDgxNjcsImlhdCI6MTc3OTcyMTc2Nywic3VuIjoidnJtVXNlciJ9.ImzGoYLex6Stb0AXWUMeFwgNzjNg1r5IL4IOxlUCiMc" \
  "http://localhost:6820/slurm/v0.0.41/ping"

# Should look like this: 
# *   Trying 127.0.0.1:6820...
# * Connected to localhost (127.0.0.1) port 6820 (#0)
# > GET /slurm/v0.0.41/ping HTTP/1.1
# > Host: localhost:6820
# > User-Agent: curl/7.81.0
# > Accept: */*
# > X-SLURM-USER-NAME: vrmUser
# > X-SLURM-USER-TOKEN: <<YOUR-ACCESS-TOKEN>>
# > 
# * Mark bundle as not supporting multiuse
# < HTTP/1.1 200 OK
# < Content-Length: 698
# < Content-Type: application/json
# < 
# {
#   "pings": [
#     {
#       "hostname": "slurmctld",
#       "pinged": "UP",
#       "latency": 1425,
#       "mode": "primary"
#     }
#   ],
#  ...
# }
```

##### Step 5: Verify Container Configuration (Optional)

```bash
# Verify Infrastructural Health
sudo docker compose ps

# Should look like this: 
# NAME         IMAGE                          COMMAND                  SERVICE      CREATED             STATUS                       PORTS
# c0           slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   c0           About an hour ago   Up About an hour (healthy)   6818/tcp
# c1           slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   c1           About an hour ago   Up About an hour (healthy)   6818/tcp
# c2           slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   c2           About an hour ago   Up About an hour (healthy)   6818/tcp
# c3           slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   c3           About an hour ago   Up About an hour (healthy)   6818/tcp
# c4           slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   c4           About an hour ago   Up About an hour (healthy)   6818/tcp
# c5           slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   c5           About an hour ago   Up About an hour (healthy)   6818/tcp
# c6           slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   c6           About an hour ago   Up About an hour (healthy)   6818/tcp
# mysql        mariadb:12                     "docker-entrypoint.s…"   mysql        About an hour ago   Up About an hour (healthy)   3306/tcp
# slurmctld    slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   slurmctld    About an hour ago   Up About an hour (healthy)   6817/tcp
# slurmdbd     slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   slurmdbd     About an hour ago   Up About an hour (healthy)   6819/tcp
# slurmrestd   slurm-docker-cluster:24.11.6   "/usr/local/bin/dock…"   slurmrestd   About an hour ago   Up About an hour (healthy)   0.0.0.0:6820->6820/tcp, [::]:6820->6820/tcp


# Verify Slurm Controller Responsiveness
sudo docker exec -it slurmctld scontrol ping

# Should look like this: 
# Slurmctld(primary) at slurmctld is UP


# Check Cluster Partition and Node Status
sudo docker exec -it slurmctld sinfo

# Should look like this: 
# PARTITION AVAIL  TIMELIMIT  NODES  STATE NODELIST
# normal*      up   infinite      6   idle c[1-6]

# Verify vrmUser Account
sudo docker exec -it slurmctld sacctmgr show association user=vrmUser

# Should look like this: 
# Cluster    Account       User  Partition     Share   Priority GrpJobs       GrpTRES GrpSubmit     GrpWall   GrpTRESMins MaxJobs       MaxTRES MaxTRESPerNode MaxSubmit     MaxWall   MaxTRESMins                  QOS   Def QOS GrpTRESRunMin 
# ---------- ---------- ---------- ---------- --------- ---------- ------- ------------- --------- ----------- ------------- ------- ------------- -------------- --------- ----------- ------------- -------------------- --------- ------------- 


# Verify Shared Storage Volumetry
# Write a file to compute component
sudo docker exec -it -u vrmUser c0 touch /data/verification_test.txt

# Verify the file is visible from the control daemon
sudo docker exec -it slurmctld ls -la /data

# Should look like this: 
# total 8
# drwxrwxrwx 2 root    root    4096 May 25 18:58 .
# drwxr-xr-x 1 root    root    4096 May 25 17:46 ..
# -rw-r--r-- 1 vrmUser vrmUser    0 May 25 17:56 sim.err
# -rw-r--r-- 1 vrmUser vrmUser    0 May 25 17:56 sim.out
# -rw-r--r-- 1 vrmUser vrmUser    0 May 25 18:58 verification_test.txt
```

##### Step 6 Configure VRM-Rust
Finally, update the project configuration to point to your new cluster. Open `VRM-Rust/data/demo/vrm_with_slurm.json` and update the following fields:
```json
{
  "userName": "vrmUser",
  "jwtToken": "<YOUR-ACCESS-TOKEN>"
}
```

##### Step 7 Run the VRM-Rust with Demo data 

```bash
cargo run -- --input-file data/workflow_with_direct_mapping.json --config-file data/vrm_with_slurm.json
```

## Project Structure (Overview)

```plaintext
├── data/                                # Configuration and input files
│   ├── demo/                            # Demo data to run the VRM-Rust system
│   └── test/                            # Test configuration and workflow data
├── src/
│   ├── error.rs                         # Centralized error types
│   ├── lib.rs                           # Library root
│   ├── main.rs                          # Binary entry point
│   ├── loader/                          # JSON file parser and configuration loader
│   ├── schema/                          # DTO (Data Transfer Object) definitions
│   ├── gui/                             # TODO 
│   └── vrm/                             # Core VRM domain logic
│       ├── vrm.rs                       # VRM root struct and lifecycle
│       ├── vrm_manager.rs               # Top-level reservation orchestration
│       ├── client/                      # Client abstraction and parsing
│       ├── common/                      # Shared utilities (Configuration, ID System, Logging and more)
│       ├── global_clock/                # System time management (GlobalClock)
│       ├── reservation/                 # Reservation types and management
│       ├── resource/                    # Resource types and management
│       ├── rms/                         # RMS adapters (AcI ↔ HPC)
│       ├── schedule/                    # Time-slotted scheduling (for Advance Reservation)
│       ├── vrm_component/               # Hierarchical VRM components (ADC/AcI)
│       └── workflow/                    # Workflow construction and management
├── tests/                               # Integration tests
├── diagrams/                            # Architecture and state diagrams
├── logs/                                # Runtime log output
└── Cargo.toml                           # Build configuration
```

## VRM-Rust Prototype Documentation: Ideas, Unimplemented Features, and Optimizations

### Ideas 

This section lists all unfinished concepts of the VRM-Rust prototype. These unfinished concepts are marked with **Idea:** in the project.

- The client should have the ability to request all of their currently scheduled reservations on the system. The current state of this feature is that the data is aggregated, but the new request type is not implemented. See the `get_managed_reservations_for_client` function in `vrm_manager.rs`. 
- A mechanism should exist that periodically deletes all reservations in the `ReservationStore` with the state Rejected, Deleted, or Finished. For reservations in the rejected state, the system must also check if the reservation has been deleted from the schedule and the RMS system (if committed or reserved). The idea is to use the `on_reservation_change` notification functionality in `vrm_state_listener.rs` to notify the `vrm_manager`, which can then add the reservation to the deletion list in the `ReservationStore`. However, the reservation should only be deleted after a grace period (which is important for debugging and tracking purposes). The idea is, that `ReservationStore` periodically checks (e.g., every 30s) which reservations can be deleted. If a reservation is on the deletion list, a reference bit is added. If it already has a reference bit, it is deleted. This ensures that the reservation is not immediately deleted. This feature must also handle the cleanup in the `VrmComponent`s. These components track the committed reservations, so if a reservation is deleted, these systems should be notified too. 

### Not Implemented 

This section lists all unimplemented functionalities of the VRM system. The corresponding code sections are marked with **Unimplemented:**. 

- All functionalities of the `SlottedLinkSchedule` regarding fragmentation calculation and creating system metrics. The mocks for these functions are located in the file `link_strategy.rs`. The aggregation functionality for the fragmentation calculation and system metrics in higher components is already implemented. All results that return **-1** are considered unimplemented by these components and are therefore treated as invalid.
- Rescheduling functionality: In case a node is taken offline, the corresponding scheduled reservations on this node must be rescheduled on different nodes. This is currently not done. The `update_nodes` function in the `ResourceStore` simply deletes all reservations associated with the corresponding node. 

### Optimizations

This section lists parts of the VRM implementation that could function as a bottleneck in later stages of the system. These sections are highlighted with **Optimization:**.