use core::f64;
use std::any::Any;
use std::collections::{HashMap, HashSet};

use crate::vrm::commons::id::{
    ClientId, CoAllocationDependencyId, CoAllocationId, DataDependencyId, ReservationName, SyncDependencyId, WorkflowNodeId,
};
use crate::error::Error;
use crate::schema::reservation_dto::{ReservationProceedingDto, ReservationStateDto};
use crate::schema::workflow_dto::{TaskDto, WorkflowDto};
use crate::vrm::reservation::link_reservation::LinkReservation;
use crate::vrm::reservation::node_reservation::NodeReservation;
use crate::vrm::reservation::reservation::{Reservation, ReservationBase, ReservationProceeding, ReservationState, ReservationTrait, ReservationTyp};
use crate::vrm::reservation::reservation_store::{ReservationId, ReservationStore};

use serde::{Deserialize, Serialize};
use union_find::{QuickUnionUf, UnionBySize, UnionFind};

use super::co_allocation::CoAllocation;
use super::dependency::{CoAllocationDependency, DataDependency, SyncDependency};
use super::workflow_node::WorkflowNode;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Workflow {
    pub base: ReservationBase,

    // The graph components, stored in HashMaps
    pub nodes: HashMap<WorkflowNodeId, WorkflowNode>,
    pub data_dependencies: HashMap<DataDependencyId, DataDependency>,
    pub sync_dependencies: HashMap<SyncDependencyId, SyncDependency>,

    // The CoAllocations are later utilized for scheduling.
    pub co_allocations: HashMap<CoAllocationId, CoAllocation>,
    pub co_allocation_dependencies: HashMap<CoAllocationDependencyId, CoAllocationDependency>,

    /// Keys to Workflow.nodes
    pub entry_nodes: Vec<WorkflowNodeId>,

    /// Keys to Workflow.nodes
    pub exit_nodes: Vec<WorkflowNodeId>,

    /// Keys to Workflow.co_allocation
    pub entry_co_allocation: Vec<CoAllocationId>,

    /// Keys to Workflow.co_allocation
    pub exit_co_allocation: Vec<CoAllocationId>,
}

// A temporary struct to hold dependencies that have a source but no target yet.
#[derive(Debug, Clone)]
enum DanglingDependency {
    Data(DataDependency),
    Sync(SyncDependency),
}

impl Workflow {
    /// Constructs a complete Workflow graph from a WorkflowDto.
    ///
    /// This is the main entry point for parsing a DTO into the internal domain model.
    /// Also builds the **CoAllocation graph**, which is later utilized for scheduling.
    pub fn create_form_dto(dto: WorkflowDto, client_id: ClientId, reservation_store: ReservationStore) -> Result<ReservationId, Error> {
        // Phase 0: Create the base workflow object
        let base = Self::build_base_workflow(&dto, client_id.clone());

        // Phase 1: Create all WorkflowNodes from the DTO tasks
        let mut nodes = Self::generate_workflow_nodes(&dto, client_id.clone(), reservation_store.clone());

        // Phase 2: Create all Data and Sync dependencies from DTO
        let (data_dependencies, sync_dependencies) = Self::build_all_dependencies(&dto, client_id, reservation_store.clone())?;

        // Phase 3: Populate the adjacency lists (incoming/outgoing) on each node
        Self::populate_node_adjacency_lists(&mut nodes, &data_dependencies, &sync_dependencies);

        // Phase 4: Build SyncGroups (co-allocation groups) using a Disjoint Set Union
        let (mut co_allocations, node_to_co_allocation) = Self::build_co_allocations(&nodes, &sync_dependencies)?;

        // Phase 5: Build the "CoAllocation Graph" of dependencies *between* SyncGroups
        let co_allocation_dependencies = Self::build_co_allocation_dependencies(&data_dependencies, &node_to_co_allocation, &mut co_allocations)?;

        // Phase 6: Find the entry/exit points for both graphs
        let (entry_nodes, exit_nodes, entry_co_allocation, exit_co_allocation) = Self::find_entry_exit_points(&nodes, &co_allocations);

        // Final-Step: Update all nodes with their final CoAllocation key
        // Also update co_allocation_key in WorkflowNodes
        for (node_id, group_id) in node_to_co_allocation {
            if let Some(node) = nodes.get_mut(&node_id) {
                node.co_allocation_key = Some(group_id.clone());
                let reservation_id_opt = node.reservation_id;

                if let Some(co_alloc) = co_allocations.get_mut(&group_id) {
                    if let Some(rep) = &mut co_alloc.representative {
                        if rep.reservation_id == reservation_id_opt {
                            rep.co_allocation_key = Some(group_id);
                        }
                    }
                }
            }
        }

        // Add the Data dependencies to all node reservations of the workflow.

        // Update ReservationStore information
        for (_, link) in &data_dependencies {
            if let Some(res_handle) = reservation_store.get(link.reservation_id) {
                let mut res = res_handle.write();
                // Add source and target node to the link reservation
                res.as_link_mut().unwrap().set_start_point(Some(link.source_node.clone().unwrap().cast()));
                res.as_link_mut().unwrap().set_end_point(Some(link.target_node.clone().unwrap().cast()));

                // Add the Data dependency to the NodeReservation
                if let Some(target_node_id) = &link.target_node {
                    if let Some(target_node) = nodes.get(target_node_id) {
                        if let Some(source_node) = nodes.get(&link.source_node.clone().unwrap()) {
                            if let Some(node_handle) = reservation_store.get(target_node.reservation_id) {
                                let mut res = node_handle.write();
                                res.as_node_mut().unwrap().data_dependencies.insert(source_node.reservation_id);
                            }
                        }
                    }
                }
            }
        }

        for (_, link) in &sync_dependencies {
            if let Some(res_handle) = reservation_store.get(link.reservation_id) {
                let mut res = res_handle.write();
                res.as_link_mut().unwrap().set_start_point(Some(link.source_node.clone().unwrap().cast()));
                res.as_link_mut().unwrap().set_end_point(Some(link.target_node.clone().unwrap().cast()));
            }
        }

        let workflow = Workflow {
            base,
            nodes,
            data_dependencies,
            sync_dependencies,
            co_allocations,
            co_allocation_dependencies,
            entry_nodes,
            exit_nodes,
            entry_co_allocation,
            exit_co_allocation,
        };

        let workflow_reservation_id = reservation_store.add(Reservation::Workflow(workflow));

        Ok(workflow_reservation_id)
    }

    /// **Phase 0: Build Base Workflow**
    ///
    /// Creates the root `ReservationBase` for the `Workflow` itself from the DTO.
    pub fn build_base_workflow(dto: &WorkflowDto, client_id: ClientId) -> ReservationBase {
        ReservationBase {
            name: ReservationName::new(dto.id.clone()),
            client_id: client_id,
            handler_id: None,
            state: dto.reservation_state.to_reservation_state(), // Workflow state is managed separately
            request_proceeding: dto.request_proceeding.to_reservation_proceeding(), // Default
            arrival_time: dto.arrival_time,
            booking_interval_start: dto.booking_interval_start,
            booking_interval_end: dto.booking_interval_end,
            assigned_start: 0,
            assigned_end: 0,
            task_duration: 0,     // Duration is an aggregate of nodes
            reserved_capacity: 0, // Capacity is an aggregate of nodes
            is_moldable: false,
            moldable_work: 0,
            frag_delta: f64::MAX,
        }
    }

    /// **Phase 1: Generate Workflow Nodes**
    pub fn generate_workflow_nodes(
        dto: &WorkflowDto,
        client_id: ClientId,
        reservation_store: ReservationStore,
    ) -> HashMap<WorkflowNodeId, WorkflowNode> {
        let mut nodes = HashMap::new();

        for task_dto in &dto.tasks {
            let node_res_dto = &task_dto.node_reservation;
            let node_id = WorkflowNodeId::new(task_dto.id.clone());
            let node_name = ReservationName::new(task_dto.id.clone());

            let node_base = ReservationBase {
                name: node_name,
                client_id: client_id.clone(),
                handler_id: None,
                state: map_reservation_state(task_dto.reservation_state),
                request_proceeding: map_reservation_proceeding(task_dto.request_proceeding),
                arrival_time: dto.arrival_time,
                booking_interval_start: dto.booking_interval_start,
                booking_interval_end: dto.booking_interval_end,
                assigned_start: 0, // Not scheduled yet
                assigned_end: 0,   // Not scheduled yet
                task_duration: node_res_dto.duration,
                reserved_capacity: node_res_dto.cpus,
                is_moldable: node_res_dto.is_moldable,
                moldable_work: node_res_dto.duration * node_res_dto.cpus,
                frag_delta: f64::MAX,
            };

            let node_reservation = NodeReservation {
                data_dependencies: HashSet::new(),
                base: node_base,
                current_working_directory: node_res_dto.current_working_directory.clone(),
                environment: node_res_dto.environment.clone(),
                task_path: node_res_dto.task_path.clone(),
                output_path: node_res_dto.output_path.clone(),
                error_path: node_res_dto.error_path.clone(),
            };

            // Add to reservation_store
            let reservation_id = reservation_store.add(Reservation::Node(node_reservation));

            // Create the WorkflowNode, data and sync links are added later
            let workflow_node = WorkflowNode {
                reservation_id,
                incoming_data: Vec::new(),
                outgoing_data: Vec::new(),
                incoming_sync: Vec::new(),
                outgoing_sync: Vec::new(),
                co_allocation_key: None, // See Phase 4
            };

            nodes.insert(node_id, workflow_node);
        }
        nodes
    }

    /// **Phase 2: Build All Dependencies**
    ///
    /// Parses the DTO to create all `DataDependency` and `SyncDependency` objects.
    /// 1. Create "dangling" dependencies from `data_out`.
    /// 2. Connect them using `data_in`.
    /// 3. Create implicit dependencies from `dependencies: { data, sync }`.
    ///
    /// Returns the complete maps of data and sync dependencies.
    pub fn build_all_dependencies(
        dto: &WorkflowDto,
        client_id: ClientId,
        reservation_store: ReservationStore,
    ) -> Result<(HashMap<DataDependencyId, DataDependency>, HashMap<SyncDependencyId, SyncDependency>), Error> {
        let mut data_dependencies = HashMap::new();
        let mut sync_dependencies = HashMap::new();
        let mut dangling_deps: HashMap<String, DanglingDependency> = HashMap::new();
        let workflow_id = &dto.id;

        // Phase 2.1: Process DataOut
        for task_dto in &dto.tasks {
            let source_node_id = &task_dto.id;
            let node_res_dto = &task_dto.node_reservation;

            for data_out in &node_res_dto.data_out {
                let port_name = &data_out.name;

                // Key to find the dangling dependency later
                let dangling_key = format!("{}/{}", source_node_id, port_name);

                let dep_id_str = format!("{}.{}.{}", workflow_id, source_node_id, port_name);
                let dep_name = ReservationName::new(dep_id_str.clone());

                let mut dep_base = ReservationBase {
                    name: dep_name,
                    client_id: client_id.clone(),
                    handler_id: None,
                    state: task_dto.reservation_state.to_reservation_state(),
                    request_proceeding: task_dto.request_proceeding.to_reservation_proceeding(),
                    arrival_time: dto.arrival_time,
                    booking_interval_start: dto.booking_interval_start,
                    booking_interval_end: dto.booking_interval_end,
                    assigned_start: 0,
                    assigned_end: 0,
                    task_duration: 1, // Default for links
                    reserved_capacity: 0,
                    is_moldable: false,
                    moldable_work: 0,
                    frag_delta: f64::MAX,
                };

                // DataDependency (file transfer)
                if let Some(size) = data_out.size {
                    dep_base.is_moldable = true;
                    dep_base.reserved_capacity = size;
                    dep_base.moldable_work = size * dep_base.task_duration;
                    let link_res = LinkReservation { base: dep_base, start_point: None, end_point: None };
                    let reservation_id = reservation_store.add(Reservation::Link(link_res));

                    let data_dep = DataDependency {
                        reservation_id,
                        source_node: Some(WorkflowNodeId::new(source_node_id.clone())),
                        target_node: None,
                        port_name: port_name.clone(),
                        size,
                    };
                    dangling_deps.insert(dangling_key, DanglingDependency::Data(data_dep));
                }
                // SyncDependency
                else if let Some(bandwidth) = data_out.bandwidth {
                    dep_base.is_moldable = false;
                    dep_base.reserved_capacity = bandwidth;
                    dep_base.moldable_work = bandwidth * dep_base.task_duration;
                    let link_res = LinkReservation { base: dep_base, start_point: None, end_point: None };
                    let reservation_id = reservation_store.add(Reservation::Link(link_res));

                    let sync_dep = SyncDependency {
                        reservation_id,
                        source_node: Some(WorkflowNodeId::new(source_node_id.clone())),
                        target_node: None,
                        port_name: port_name.clone(),
                        bandwidth,
                    };
                    dangling_deps.insert(dangling_key, DanglingDependency::Sync(sync_dep));
                }
            }
        }

        // Phase 2.2: Process DataIn
        for task_dto in &dto.tasks {
            let target_node_id = WorkflowNodeId::new(task_dto.id.clone());
            let node_res_dto = &task_dto.node_reservation;

            for data_in in &node_res_dto.data_in {
                let dangling_key = format!("{}/{}", data_in.source_reservation, data_in.source_port);

                if let Some(dangling_dep) = dangling_deps.remove(&dangling_key) {
                    match dangling_dep {
                        DanglingDependency::Data(mut data_dep) => {
                            data_dep.target_node = Some(target_node_id.clone());
                            let name = reservation_store.get_name_for_key(data_dep.reservation_id).unwrap();
                            let dep_id = DataDependencyId::new(name.id);
                            data_dependencies.insert(dep_id, data_dep);
                        }
                        DanglingDependency::Sync(mut sync_dep) => {
                            sync_dep.target_node = Some(target_node_id.clone());
                            let name = reservation_store.get_name_for_key(sync_dep.reservation_id).unwrap();
                            let dep_id = SyncDependencyId::new(name.id);
                            sync_dependencies.insert(dep_id, sync_dep);
                        }
                    }
                } else {
                    // Dependency source not found!"
                    log::warn!("Could not find source for DataIn: {}", dangling_key);
                }
            }
        }

        // Phase 2.3: Process Dependencies (data/sync)
        for task_dto in &dto.tasks {
            let target_node_id = &task_dto.id;
            let dep_dto = &task_dto.node_reservation.dependencies;

            // "data" are DataDependencies with size 0
            Self::create_implicit_dependencies(
                workflow_id,
                &dep_dto.data,
                target_node_id,
                task_dto,
                dto.arrival_time,
                dto.booking_interval_start,
                dto.booking_interval_end,
                &mut data_dependencies,
                &mut sync_dependencies,
                "data",
                client_id.clone(),
                reservation_store.clone(),
            );

            // "sync" are SyncDependencies with bandwidth 0
            Self::create_implicit_dependencies(
                workflow_id,
                &dep_dto.sync,
                target_node_id,
                task_dto,
                dto.arrival_time,
                dto.booking_interval_start,
                dto.booking_interval_end,
                &mut data_dependencies,
                &mut sync_dependencies,
                "sync",
                client_id.clone(),
                reservation_store.clone(),
            );
        }

        Ok((data_dependencies, sync_dependencies))
    }

    /// **Phase 2.3 Helper:** Creates implicit "data" (Data) and "sync" (Sync) dependencies.
    #[allow(clippy::too_many_arguments)]
    pub fn create_implicit_dependencies(
        workflow_id: &str,
        source_ids: &[String],
        target_node_id: &str,
        task_dto: &TaskDto,
        arrival_time: i64,
        booking_start: i64,
        booking_end: i64,
        data_deps: &mut HashMap<DataDependencyId, DataDependency>,
        sync_deps: &mut HashMap<SyncDependencyId, SyncDependency>,
        dep_type: &str,
        client_id: ClientId,
        reservation_store: ReservationStore,
    ) {
        for source_id in source_ids {
            let dep_id_str = format!("{}.{}.{}.{}", workflow_id, dep_type, source_id, target_node_id);

            let dep_base = ReservationBase {
                name: ReservationName::new(dep_id_str.clone()),
                client_id: client_id.clone(),
                handler_id: None,
                state: ReservationState::Open,
                request_proceeding: map_reservation_proceeding(task_dto.request_proceeding),
                arrival_time,
                booking_interval_start: booking_start,
                booking_interval_end: booking_end,
                assigned_start: 0,
                assigned_end: 0,
                task_duration: 0,
                reserved_capacity: 0,
                is_moldable: false,
                moldable_work: 0,
                frag_delta: f64::MAX,
            };
            let link_res = LinkReservation { base: dep_base, start_point: None, end_point: None };
            let reservation_id = reservation_store.add(Reservation::Link(link_res));

            if dep_type == "data" {
                let data_dep = DataDependency {
                    reservation_id,
                    source_node: Some(WorkflowNodeId::new(source_id.clone())),
                    target_node: Some(WorkflowNodeId::new(target_node_id.to_string())),
                    port_name: "data".to_string(),
                    size: 0,
                };
                data_deps.insert(DataDependencyId::new(dep_id_str), data_dep);
            } else if dep_type == "sync" {
                let sync_dep = SyncDependency {
                    reservation_id,
                    source_node: Some(WorkflowNodeId::new(source_id.clone())),
                    target_node: Some(WorkflowNodeId::new(target_node_id.to_string())),
                    port_name: "sync".to_string(),
                    bandwidth: 0,
                };
                sync_deps.insert(SyncDependencyId::new(dep_id_str), sync_dep);
            }
        }
    }

    /// **Phase 3: Populate Node Adjacency Lists**
    ///
    /// Connects the `WorkflowNode`s by populating their `incoming_` and `outgoing_`
    /// `Vec`s with the dependency IDs.
    pub fn populate_node_adjacency_lists(
        nodes: &mut HashMap<WorkflowNodeId, WorkflowNode>,
        data_dependencies: &HashMap<DataDependencyId, DataDependency>,
        sync_dependencies: &HashMap<SyncDependencyId, SyncDependency>,
    ) {
        for (dep_id, data_dep) in data_dependencies {
            if let Some(ref source_id) = data_dep.source_node {
                if let Some(source_node) = nodes.get_mut(source_id) {
                    source_node.outgoing_data.push(dep_id.clone());
                } else {
                    log::warn!("DataDep source node '{}' not found for dep '{}'", source_id, dep_id);
                }
            }
            if let Some(ref target_id) = data_dep.target_node {
                if let Some(target_node) = nodes.get_mut(target_id) {
                    target_node.incoming_data.push(dep_id.clone());
                } else {
                    log::warn!("DataDep target node '{}' not found for dep '{}'", target_id, dep_id);
                }
            }
        }

        for (dep_id, sync_dep) in sync_dependencies {
            if let Some(ref source_id) = sync_dep.source_node {
                if let Some(source_node) = nodes.get_mut(source_id) {
                    source_node.outgoing_sync.push(dep_id.clone());
                } else {
                    log::warn!("SyncDep source node '{}' not found for dep '{}'", source_id, dep_id);
                }
            }
            if let Some(ref target_id) = sync_dep.target_node {
                if let Some(target_node) = nodes.get_mut(target_id) {
                    target_node.incoming_sync.push(dep_id.clone());
                } else {
                    log::warn!("SyncDep target node '{}' not found for dep '{}'", target_id, dep_id);
                }
            }
        }
    }

    /// **Phase 4: Build CoAllocation Graph**
    ///
    /// Identifies co-allocation groups. It uses a Disjoint Set Union (DSU) structure
    /// to merge nodes that are connected by `SyncDependency`.
    pub fn build_co_allocations(
        nodes: &HashMap<WorkflowNodeId, WorkflowNode>,
        sync_dependencies: &HashMap<SyncDependencyId, SyncDependency>,
    ) -> Result<(HashMap<CoAllocationId, CoAllocation>, HashMap<WorkflowNodeId, CoAllocationId>), Error> {
        let mut preliminary_co_allocation: HashMap<WorkflowNodeId, CoAllocation> = HashMap::new();
        let mut node_to_co_allocation: HashMap<WorkflowNodeId, CoAllocationId> = HashMap::new();

        // 1. Create mappings between String IDs and usize indices for the DSU
        let node_ids: Vec<WorkflowNodeId> = nodes.keys().cloned().collect();
        let mut node_id_to_index: HashMap<WorkflowNodeId, usize> = HashMap::with_capacity(node_ids.len());
        for (index, id) in node_ids.iter().enumerate() {
            node_id_to_index.insert(id.clone(), index);
        }

        // 2. Initialize the DSU structure
        let mut dsu = QuickUnionUf::<UnionBySize>::new(node_ids.len());

        for (node_id, node) in nodes {
            let co_allocation_id = CoAllocationId::new(node_id.id.clone());
            let co_allocation = CoAllocation {
                id: co_allocation_id,
                representative: Some(node.clone()),
                members: vec![node_id.clone()],
                sync_dependencies: Vec::new(),
                outgoing_co_allocation_dependencies: Vec::new(),
                outgoing_data_dependencies: Vec::new(),
                incoming_co_allocation_dependencies: Vec::new(),
                incoming_data_dependencies: Vec::new(),
                rank_upward: 0,
                rank_downward: 0,
                number_of_nodes_critical_path_downwards: 0,
                number_of_nodes_critical_path_upwards: 0,
                is_in_queue: false,
                unprocessed_predecessor_count: 0,
                unprocessed_successor_count: 0,
                spare_time: 0,
                max_successor_force: 0.0,
                max_pred_force: 0.0,
                is_discovered: false,
                is_processed: false,
                is_moveable: true,
                is_moveable_interval_start: true,
                is_moveable_interval_end: true,
                start_position: 0.0,
                end_position: 0.0,
            };
            preliminary_co_allocation.insert(node_id.clone(), co_allocation);
        }

        // 3. Perform the `union` operation for every SyncDependency
        for sync_dep in sync_dependencies.values() {
            if let (Some(source_id), Some(target_id)) = (&sync_dep.source_node, &sync_dep.target_node) {
                if let (Some(&source_index), Some(&target_index)) = (node_id_to_index.get(source_id), node_id_to_index.get(target_id)) {
                    dsu.union(source_index, target_index);
                } else {
                    log::warn!("Could not find node index for SyncDependency: {} -> {}", source_id, target_id);
                }
            }
        }

        // 4. Create the final, merged CoAllocations
        let mut co_allocation: HashMap<CoAllocationId, CoAllocation> = HashMap::new();

        // Temporary map to hold merged CoAllocations by the representative id
        let mut merged_groups: HashMap<WorkflowNodeId, CoAllocation> = HashMap::new();

        for node_id in &node_ids {
            let node_index = node_id_to_index.get(node_id).expect("Node must have an index");
            let rep_index = dsu.find(*node_index);
            let rep_node_id = node_ids.get(rep_index).expect("Representative index must be valid").clone();
            let final_group_id = CoAllocationId::new(rep_node_id.id.clone());

            node_to_co_allocation.insert(node_id.clone(), final_group_id.clone());

            // If we haven't created the group for this representative yet, extract it from preliminary
            if !merged_groups.contains_key(&rep_node_id) {
                if let Some(mut base_group) = preliminary_co_allocation.remove(&rep_node_id) {
                    base_group.id = final_group_id.clone();
                    merged_groups.insert(rep_node_id.clone(), base_group);
                }
            }

            // If this node is not the representative, merge it into the representative's group
            if node_id != &rep_node_id {
                if let Some(mut prelim_group) = preliminary_co_allocation.remove(node_id) {
                    if let Some(final_group) = merged_groups.get_mut(&rep_node_id) {
                        final_group.members.append(&mut prelim_group.members);
                    }
                }
            }
        }

        // Move from merged_groups (NodeId key) to co_allocation (CoAllocationId key)
        for (_, group) in merged_groups {
            co_allocation.insert(group.id.clone(), group);
        }

        // 5. Populate the `sync_dependencies` Vec within each CoAllocation
        for (dep_id, sync_dep) in sync_dependencies {
            if let Some(ref source_id) = sync_dep.source_node {
                let co_alloc_id = node_to_co_allocation.get(source_id).expect("Node must be in a CoAllocation");
                if let Some(group) = co_allocation.get_mut(co_alloc_id) {
                    group.sync_dependencies.push(sync_dep.clone());
                } else {
                    log::warn!("CoAllocation {} not found for SyncDependency {}", co_alloc_id, dep_id);
                }
            }
        }

        Ok((co_allocation, node_to_co_allocation))
    }

    /// **Phase 5: Build CoAllocation Dependencies (Co-Allocation Graph)**
    ///
    /// Creates the "CoAllocation graph" by building `CoAllocationDependency` edges
    /// *between* the `CoAllocation`s.
    ///
    /// This iterates over all `DataDependency`s and, if a dependency links
    /// nodes in two *different* `CoAllocation`s, its creates an edge between those groups.
    pub fn build_co_allocation_dependencies(
        data_dependencies: &HashMap<DataDependencyId, DataDependency>,
        node_to_co_allocation: &HashMap<WorkflowNodeId, CoAllocationId>,
        co_allocation: &mut HashMap<CoAllocationId, CoAllocation>,
    ) -> Result<HashMap<CoAllocationDependencyId, CoAllocationDependency>, Error> {
        let mut co_allocation_dependencies = HashMap::new();

        for (dep_id, data_dep) in data_dependencies {
            if let (Some(source_node), Some(target_node)) = (&data_dep.source_node, &data_dep.target_node) {
                if let (Some(source_co_allocation_id), Some(target_co_allocation_id)) =
                    (node_to_co_allocation.get(source_node), node_to_co_allocation.get(target_node))
                {
                    // Only create sync group edges between *different* CoAllocations
                    if source_co_allocation_id != target_co_allocation_id {
                        let co_allocation_dep_id = CoAllocationDependencyId::new(dep_id.id.clone());
                        let co_allocation_dep = CoAllocationDependency {
                            id: co_allocation_dep_id.clone(),
                            source_group: source_co_allocation_id.clone(),
                            target_group: target_co_allocation_id.clone(),
                            data_dependency: dep_id.clone(),
                        };

                        co_allocation_dependencies.insert(co_allocation_dep_id, co_allocation_dep.clone());

                        // Correctly populate the adjacency lists in the CoAllocation
                        if let Some(source_co_allocation) = co_allocation.get_mut(source_co_allocation_id) {
                            source_co_allocation.outgoing_co_allocation_dependencies.push(co_allocation_dep.clone());
                            source_co_allocation.outgoing_data_dependencies.push(data_dep.clone());
                        }
                        if let Some(target_co_allocation) = co_allocation.get_mut(target_co_allocation_id) {
                            target_co_allocation.incoming_co_allocation_dependencies.push(co_allocation_dep);
                            target_co_allocation.incoming_data_dependencies.push(data_dep.clone());
                        }
                    }
                } else {
                    log::warn!(
                        "Skipping co_allocation dependency '{}' because source ('{}') or target ('{}') node was not found in co_allocation map.",
                        dep_id,
                        source_node,
                        target_node
                    );
                }
            }
        }
        Ok(co_allocation_dependencies)
    }

    /// **Phase 6: Find Entry and Exit Points**
    ///
    /// Finds all entry/exit nodes for the base graph and all entry/exit
    /// groups for the CoAllocation graph. This is used by the scheduler to find
    /// the starting points for traversal.
    pub fn find_entry_exit_points(
        nodes: &HashMap<WorkflowNodeId, WorkflowNode>,
        co_allocation: &HashMap<CoAllocationId, CoAllocation>,
    ) -> (Vec<WorkflowNodeId>, Vec<WorkflowNodeId>, Vec<CoAllocationId>, Vec<CoAllocationId>) {
        let entry_nodes = nodes.iter().filter(|(_, n)| n.incoming_data.is_empty() && n.incoming_sync.is_empty()).map(|(k, _)| k.clone()).collect();

        let exit_nodes = nodes.iter().filter(|(_, n)| n.outgoing_data.is_empty() && n.outgoing_sync.is_empty()).map(|(k, _)| k.clone()).collect();

        // Find Entry/Exit SyncGroups based on the *overlay* graph
        let entry_co_allocation =
            co_allocation.values().filter(|on| on.incoming_co_allocation_dependencies.is_empty()).map(|on| on.id.clone()).collect();

        let exit_co_allocation =
            co_allocation.values().filter(|on| on.outgoing_co_allocation_dependencies.is_empty()).map(|on| on.id.clone()).collect();

        (entry_nodes, exit_nodes, entry_co_allocation, exit_co_allocation)
    }
}

pub fn map_reservation_state(dto_state: ReservationStateDto) -> ReservationState {
    match dto_state {
        ReservationStateDto::Rejected => ReservationState::Rejected,
        ReservationStateDto::Deleted => ReservationState::Deleted,
        ReservationStateDto::Open => ReservationState::Open,
        ReservationStateDto::ProbeAnswer => ReservationState::ProbeAnswer,
        ReservationStateDto::ReserveAnswer => ReservationState::ReserveAnswer,
        ReservationStateDto::Committed => ReservationState::Committed,
        ReservationStateDto::Finished => ReservationState::Finished,
    }
}

pub fn map_reservation_proceeding(dto_proc: ReservationProceedingDto) -> ReservationProceeding {
    match dto_proc {
        ReservationProceedingDto::Probe => ReservationProceeding::Probe,
        ReservationProceedingDto::Reserve => ReservationProceeding::Reserve,
        ReservationProceedingDto::Commit => ReservationProceeding::Commit,
        ReservationProceedingDto::Delete => ReservationProceeding::Delete,
    }
}

impl Workflow {
    /// Computes the upward rank for all `CoAllocation`s in the Workflow.
    ///
    /// The upward rank (`rank_upward`) is the length of the longest path through the workflow.
    ///
    /// This function also computes the number of nodes in the critical upward path
    /// (`number_of_nodes_critical_path_upwards`) for all nodes.
    ///
    /// A `Vec<Option<WorkflowNode>>` containing the `representative` node for
    /// every `CoAllocation` in the workflow, ordered by `rank_upward` in descending
    /// order (largest ranks are first).
    pub fn calculate_upward_rank(&mut self, avg_net_speed: i64, reservation_store: &ReservationStore) -> Vec<WorkflowNode> {
        let mut finished_node_keys: Vec<CoAllocationId> = Vec::with_capacity(self.co_allocations.len());
        let mut queue: Vec<CoAllocationId> = Vec::new();

        // 1. Clear marks on all Nodes
        for co_allocation in self.co_allocations.values_mut() {
            co_allocation.is_discovered = false;
            co_allocation.is_processed = false;
        }

        // 2. Initiate queue with the entry nodes
        for group_key in &self.entry_co_allocation {
            if let Some(entry_group) = self.co_allocations.get_mut(group_key) {
                entry_group.is_discovered = true;
                queue.push(group_key.clone());
            }
        }

        // 3. Compute the ranks of every node
        while let Some(next_key) = queue.last() {
            let next_key = next_key.clone();
            let node = self.co_allocations.get(&next_key).unwrap_or_else(|| panic!("CoAllocation key '{}' in queue but not in map.", next_key));
            if node.is_processed {
                queue.pop();
                continue;
            }

            let node_duration = node.get_co_allocation_duration(&self.nodes, &reservation_store);
            let outgoing_deps = node.outgoing_co_allocation_dependencies.clone();
            let mut rank = node_duration;
            let mut number_of_nodes_critical_path = 1;
            let mut is_successor_without_rank = false;

            for outgoing_dep in &outgoing_deps {
                let target_key = &outgoing_dep.target_group;

                if let Some(target_group) = self.co_allocations.get(target_key) {
                    if !target_group.is_processed {
                        is_successor_without_rank = true;
                        queue.push(target_key.clone());
                    } else {
                        let size = self.data_dependencies.get(&outgoing_dep.data_dependency).expect("Data dependency not found").size;

                        let communication_time = if avg_net_speed > 0 {
                            size / avg_net_speed
                        } else {
                            log::warn!("avg_net_speed is 0, setting communication_time to 0");
                            0
                        };

                        let successor_rank = target_group.rank_upward;
                        let new_possible_rank = node_duration + communication_time + successor_rank;

                        if rank < new_possible_rank {
                            rank = new_possible_rank;
                            number_of_nodes_critical_path = target_group.number_of_nodes_critical_path_upwards + 1;
                        }
                    }
                } else {
                    log::warn!("Target CoAllocation '{}' not found.", target_key);
                }
            }

            // 5. Calculate rank for this node if all successors are processed
            if !is_successor_without_rank {
                let processed_node = self.co_allocations.get_mut(&next_key).expect("Node must exist");

                processed_node.rank_upward = rank;
                processed_node.number_of_nodes_critical_path_upwards = number_of_nodes_critical_path;
                processed_node.is_processed = true;

                queue.pop();
                finished_node_keys.push(next_key);
            }
        }

        // 6. Build sortedList
        finished_node_keys.sort_by(|a_key, b_key| {
            let a_rank = self.co_allocations.get(a_key).unwrap().rank_upward;
            let b_rank = self.co_allocations.get(b_key).unwrap().rank_upward;
            b_rank.cmp(&a_rank)
        });

        // 7. Map keys to the representative nodes
        return finished_node_keys.into_iter().map(|key| self.co_allocations.get(&key).unwrap().representative.clone().unwrap()).collect();
    }

    /// Computes the downward rank for all `CoAllocation`s in the Workflow.
    ///
    /// The downward rank (`rank_downward`) is the length of the longest path through the workflow (starting at an entry node).
    ///
    /// This function also computes the number of nodes in the critical downward path
    /// (`number_of_nodes_critical_path_downwards`) for all nodes.
    ///
    /// A `Vec<Option<WorkflowNode>>` containing the `representative` node for
    /// every `CoAllocation` in the workflow, ordered by `rank_downward` in descending
    /// order (largest ranks are first).
    fn _calculate_downward_rank(mut self, avg_net_speed: i64, reservation_store: ReservationStore) -> Vec<Option<WorkflowNode>> {
        let mut finished_node_keys: Vec<CoAllocationId> = Vec::with_capacity(self.co_allocations.len());
        let mut queue: Vec<CoAllocationId> = Vec::new();

        for co_allocation in self.co_allocations.values_mut() {
            co_allocation.is_discovered = false;
            co_allocation.is_processed = false;
        }

        for group_key in &self.exit_co_allocation {
            if let Some(exit_group) = self.co_allocations.get_mut(group_key) {
                exit_group.is_discovered = true;
                queue.push(group_key.clone());
            }
        }

        while let Some(next_key) = queue.last() {
            let next_key = next_key.clone();

            let node = self.co_allocations.get(&next_key).unwrap_or_else(|| panic!("CoAllocation key '{}' in queue but not in map.", next_key));

            if node.is_processed {
                queue.pop();
                continue;
            }

            let node_duration = node.get_co_allocation_duration(&self.nodes, &reservation_store);
            let incoming_deps = node.incoming_co_allocation_dependencies.clone();

            let mut rank = node_duration;
            let mut number_of_nodes_critical_path = 1;
            let mut is_predecessor_without_rank = false;

            for incoming_dep in &incoming_deps {
                let source_key = &incoming_dep.source_group;

                if let Some(source_group) = self.co_allocations.get(source_key) {
                    if !source_group.is_processed {
                        is_predecessor_without_rank = true;

                        queue.push(source_key.clone());
                    } else {
                        let size = self.data_dependencies.get(&incoming_dep.data_dependency).expect("Data dependency not found").size;

                        let communication_time = if avg_net_speed > 0 {
                            size / avg_net_speed
                        } else {
                            log::warn!("avg_net_speed is 0, setting communication_time to 0");
                            0
                        };

                        let predecessor_rank = source_group.rank_downward;
                        let new_possible_rank = node_duration + communication_time + predecessor_rank;

                        if rank < new_possible_rank {
                            rank = new_possible_rank;
                            number_of_nodes_critical_path = source_group.number_of_nodes_critical_path_downwards + 1;
                        }
                    }
                } else {
                    log::warn!("Source CoAllocation '{}' not found.", source_key);
                }
            }

            if !is_predecessor_without_rank {
                let processed_node = self.co_allocations.get_mut(&next_key).expect("Node must exist");

                processed_node.rank_downward = rank;
                processed_node.number_of_nodes_critical_path_downwards = number_of_nodes_critical_path;
                processed_node.is_processed = true;

                queue.pop();
                finished_node_keys.push(next_key);
            }
        }

        finished_node_keys.sort_by(|a_key, b_key| {
            let a_rank = self.co_allocations.get(a_key).unwrap().rank_downward;
            let b_rank = self.co_allocations.get(b_key).unwrap().rank_downward;
            b_rank.cmp(&a_rank)
        });

        return finished_node_keys.into_iter().map(|key| self.co_allocations.get(&key).unwrap().representative.clone()).collect();
    }
}

impl ReservationTrait for Workflow {
    fn get_base(&self) -> &ReservationBase {
        &self.base
    }

    fn get_base_mut(&mut self) -> &mut ReservationBase {
        &mut self.base
    }

    fn box_clone(&self) -> Box<dyn ReservationTrait> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_type(&self) -> ReservationTyp {
        ReservationTyp::Workflow
    }
}

impl Workflow {
    /**
     * Updates the Request, represented by a WorkflowNode or a Dependency, belonging to
     * the given Reservation with the data of the given Reservation.
     * @param res Reservation belonging to a Request(Reservation) in the Workflow
     */
    pub fn update_reservation(&mut self, reservation_store: ReservationStore, reservation_id: ReservationId) {
        match reservation_store.get_type(reservation_id) {
            Some(ReservationTyp::Link) => {
                self.update_workflow_assigned_start_and_end(reservation_store.clone(), reservation_id);
            }
            Some(ReservationTyp::Node) => {
                self.update_workflow_assigned_start_and_end(reservation_store.clone(), reservation_id);
            }
            _ => log::error!("Unknown Reservation type for update."),
        }
    }

    /**
     * Updates the assigned start and/or end of the workflow, if the assigned start/end
     * of the given reservation exceeds those interval
     * @param res Reservation represented by a Request(Reservation) in the Workflow
     */
    fn update_workflow_assigned_start_and_end(&mut self, reservation_store: ReservationStore, reservation_id: ReservationId) {
        if self.base.assigned_start == i64::MIN
            || (reservation_store.get_assigned_start(reservation_id) < self.base.assigned_start
                && reservation_store.get_assigned_start(reservation_id) != i64::MIN)
        {
            self.base.set_assigned_start(reservation_store.get_assigned_start(reservation_id));
        }

        if self.base.assigned_end == i64::MIN || reservation_store.get_assigned_end(reservation_id) > self.base.assigned_end {
            self.base.set_assigned_end(reservation_store.get_assigned_end(reservation_id));
        }
    }

    /**
     * @return 	List of all Requests(Reservations) in this Workflow. That are all Reservations represented
     * 			by WorkflowNodes and Dependencies.
     */
    pub fn get_all_reservation_ids(&self) -> Vec<ReservationId> {
        let mut workflow_res_ids = Vec::new();

        for node in self.nodes.values() {
            workflow_res_ids.push(node.reservation_id.clone());
        }

        for data_dep in self.data_dependencies.values() {
            workflow_res_ids.push(data_dep.reservation_id.clone());
        }

        for sync_dep in self.sync_dependencies.values() {
            workflow_res_ids.push(sync_dep.reservation_id.clone());
        }

        return workflow_res_ids;
    }
}
