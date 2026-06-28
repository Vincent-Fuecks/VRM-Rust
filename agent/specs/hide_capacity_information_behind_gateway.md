# Hide Capacity Information Behind Gateway

## Status
**Deferred** — spun off from [`decoupling_workflow_from_vrm_hardware_topology.md`](decoupling_workflow_from_vrm_hardware_topology.md) (AD-6).

## Context

When AcI components run directly on HPC sites, the total infrastructure of the HPC site may be unknown to the central VRM system. The gateway node abstracts the internal topology from the outside — currently this abstraction covers topology structure (node identities, switches, links), but still exposes some capacity details.

The current `ResourceStore.can_handle_node_request()` at the AcI level checks whether *any* internal node has sufficient CPU capacity for a given request. While the AcI does not reveal *which* node has capacity, the very fact that feasibility checks are performed at the node level means the ADC indirectly learns about the internal capacity distribution (e.g., by probing different CPU amounts and observing accept/reject patterns).

## Goal

Harden the information hiding boundary so that the AcI:
1. Reports **only aggregate capacity** to higher layers (already done via `get_total_node_capacity()`)
2. Accepts or rejects node reservations **without exposing per-node capacity information** through feasibility probe patterns
3. The ADC schedules against the gateway abstraction without any visibility into internal node count, per-node capacity, or node identities

## Potential Approaches (to be evaluated)

- **Capacity Budget Model:** The AcI maintains a "virtual capacity" budget derived from the gateway's egress/ingress limits. The ADC schedules against this budget, and the AcI internally maps virtual allocations to physical nodes.
- **Opaque Accept/Reject:** The AcI answers feasibility probes with a simple yes/no and a set of time slots, without revealing *why* a request is feasible or infeasible.
- **Homogenized Resource View:** The AcI presents its internal resources as a single homogeneous pool (total CPUs, total memory) rather than a collection of individual nodes.

## Dependencies

- Requires the gateway infrastructure from [`decoupling_workflow_from_vrm_hardware_topology.md`](decoupling_workflow_from_vrm_hardware_topology.md) to be in place first.

## Acceptance Criteria

TBD — to be defined during technical review of this US.
