# Resource Module — Data Flow

## 1. Resource Initialization Lifecycle

### Topology Setup (AcI Initialization)

```
External RMS Config (links, nodes)
         │
         ▼
NetworkTopology::new()
         │
         ├── setup_network_links()
         │       │
         │       ├── For each Link DTO:
         │       │    ├── Create SlottedScheduleContext<NodeStrategy>
         │       │    ├── Create LinkResource
         │       │    └── resource_store.add_link(link_resource)  ──►  LinkResourceId
         │       │
         │       └── Return HashSet<LinkResourceId>
         │
         ├── setup_routers()
         │       │
         │       ├── For each Node DTO: Register as grid access point
         │       ├── For each Link: Register intermediate routers
         │       └── Return HashMap<RouterId, Router>
         │
         ├── resource_store.add_routers(router_ids)
         │
         ├── setup_adjacency_matrix() ──► HashMap<RouterId, HashSet<LinkResourceId>>
         │
         └── calc_all_paths()
                 │
                 ├── For each router pair (A, B):
                 │    ├── calc_k_shortest_paths(A, B)
                 │    │    ├── BFS traversal over adjacency matrix
                 │    │    ├── Collect up to K=10 Path structs
                 │    │    └── Compute bottleneck bandwidth
                 │    │
                 │    ├── Insert into path_cache: HashMap<(RouterId, RouterId), Vec<Path>>
                 │    └── Create VirtualLinkResource (aggregated metrics)
                 │
                 └── resource_store.add_k_shortest_paths(path_cache)
```

### Node Synchronization (Runtime Update)

```
RMS Node Status Update (nodes_in_rms: Vec<NodeResource>)
         │
         ▼
ResourceStore::update_nodes(nodes_in_rms)
         │
         ├── Clone current node_index from guard
         │
         ├── For each node in nodes_in_rms:
         │    ├── If NOT in current_store_nodes → self.add_node(node)
         │    │    ├── nodes.insert(Arc::new(RwLock::new(node)))  ──►  NodeResourceId
         │    │    └── node_index.insert(name, node_resource_id)
         │    └── Else → Remove from current_store_nodes (already present)
         │
         └── For each remaining (stale) entry in current_store_nodes:
              └── self.remove_node(resource_name)
                   ├── node_index.remove(&name) ──► Option<NodeResourceId>
                   └── nodes.remove(node_resource_id)
```

## 2. Admission Control / Feasibility Check Data Flow

There are two entry points for feasibility checks, serving different consumer components:

### ADC Path: `can_handle_adc_request(Reservation)`

```
Reservation (value type)
         │
         ▼
ResourceStore::can_handle_adc_request(res)
         │
         ├── Match on Reservation enum:
         │
         ├── Reservation::Link(link_res) ──► Tagged union
         │    ├── Check (start_point, end_point) are Some
         │    └── can_handle_link_request(source, target, is_moldable, capacity)
         │
         ├── Reservation::Node(node_res) ──► Tagged union
         │    └── can_handle_node_request(&FeasibilityRequest::Node{...})
         │
         └── Reservation::Workflow(_) ──► Log error, return false
```

### AcI Path: `can_handle_aci_request(&ReservationStore, ReservationId)`

```
ReservationId + &ReservationStore (reference)
         │
         ▼
ResourceStore::can_handle_aci_request(reservation_store, reservation_id)
         │
         ├── Query ReservationStore for type, moldable, capacity, topology
         │
         ├── If is_link:
         │    ├── get_start_point(id), get_end_point(id)
         │    └── can_handle_link_request(source, target, is_moldable, capacity)
         │
         ├── If is_node:
         │    └── can_handle_node_request(&FeasibilityRequest::Node{...})
         │
         └── Else (workflow):
              └── Log error, return false
```

### Node Feasibility Check: `can_handle_node_request()`

```
FeasibilityRequest::Node { capacity, is_moldable }
         │
         ▼
ResourceStore::can_handle_node_request(&request)
         │
         └── For each NodeResource in StoreInner.nodes:
              ├── node.read().unwrap()  (acquire read lock on NodeResource)
              └── node.can_handle_request(&request)
                   └── BaseResource::can_handle(is_moldable, capacity)
                        ├── if !moldable && capacity > 0:
                        │    return capacity <= self.capacity
                        └── else: return true
              
              If any returns true → return TRUE
              If none returns true → return FALSE
```

### Link Feasibility Check: `can_handle_link_request()`

```
(source: RouterId, target: RouterId, is_moldable: bool, capacity: i64)
         │
         ├── Early stop: source == target → return TRUE
         │
         ▼
ResourceStore::can_handle_link_request(source, target, is_moldable, capacity)
         │
         ├── get_k_shortest_paths(source, target)
         │    └── path_cache.get(&(source, target)) ──► Option<Vec<Path>>
         │
         ├── If no paths found → return FALSE
         │
         └── For each Path in k_shortest_paths:
              ├── For each link_resource_id in path.network_links:
              │    ├── guard.links.get(link_resource_id)  (read StoreInner)
              │    ├── link.read().unwrap()  (acquire LinkResource read lock)
              │    └── link.can_handle_request(FeasibilityRequest::Link{...})
              │         ├── Match source/target topology
              │         └── BaseResource::can_handle(is_moldable, capacity)
              │
              ├── If all links pass → return TRUE
              └── Path failed → try next path
              
              If no path succeeds → return FALSE
```

## 3. Link Schedule Modification Flow

### Writing to a Link's Schedule (via `with_mut_slotted_schedule_strategy`)

```
Request to modify link schedule (e.g., reserve capacity on a network path)
         │
         ▼
ResourceStore::with_mut_slotted_schedule_strategy(link_id, callback_fn)
         │
         ├── get_mut_link(link_id)
         │    ├── self.inner.write().unwrap()  (acquire write lock on StoreInner)
         │    ├── guard.links.get_mut(link_id) ──► Option<Arc<RwLock<LinkResource>>>
         │    └── Return cloned Arc
         │
         ├── link_arc.write().unwrap()  (acquire write lock on LinkResource)
         │
         └── f(&mut link.schedule)
              → Executes callback with mutable access to
                SlottedScheduleContext<NodeStrategy>
```

### Network Path Insertion Flow (during reserve)

```
LinkStrategy::insert_reservation_into_slot(ctx, requirement, slot_index, reservation_id)
         │
         ├── Get k_shortest_paths for reservation's source↔target
         │
         ├── For each path:
         │    ├── For each link_id in path.network_links:
         │    │    └── resource_store.with_mut_slotted_schedule_strategy(link_id, |schedule| {
         │    │         NodeStrategy::adjust_requirement_to_slot_capacity(schedule, ...)
         │    │    })
         │    ├── If all links can accommodate:
         │    │    │  (path is free)
         │    │    ├── For each link_id:
         │    │    │    └── resource_store.with_mut_slotted_schedule_strategy(link_id, |schedule| {
         │    │    │         NodeStrategy::insert_reservation_into_slot(schedule, ...)
         │    │    │    })
         │    │    ├── reserved_paths[reservation_id][slot_index] = path
         │    │    └── ctx.get_mut_slot(slot_index).insert_reservation(capacity, id)
         │    │
         │    └── Path not free → try next path
         │
         └── If no path available → log error, no reservation made
```

## 4. Diagnostic Data Flow

### Store Dump

```
ResourceStore::dump_store_contents()
         │
         ├── guard.inner.read().expect("RwLock poisoned")
         │
         ├── Iterate guard.links.values():
         │    ├── link.try_read()
         │    └── Log base.name, base.capacity, source, target
         │
         └── Iterate guard.nodes.values():
              ├── node.try_read()
              └── Log base.name, base.capacity
```

## 5. Summary Diagram

```
                              ┌───────────────────┐
                              │   External RMS     │
                              └────────┬──────────┘
                                       │
                           (topology config / node updates)
                                       │
                                       ▼
┌──────────────────────────────────────────────────────────────────────┐
│                        ResourceStore                                 │
│  ┌────────────┐  ┌────────────┐  ┌──────────────┐  ┌─────────────┐  │
│  │ nodes      │  │ links      │  │ path_cache   │  │ router_list │  │
│  │ (SlotMap)  │  │ (SlotMap)  │  │ (HashMap)    │  │ (HashSet)   │  │
│  └─────┬──────┘  └─────┬──────┘  └──────────────┘  └─────────────┘  │
│        │               │                                              │
│        ▼               ▼                                              │
│  ┌──────────┐   ┌──────────┐                                         │
│  │NodeResrc │   │LinkResrc │                                         │
│  │ .base    │   │ .base    │                                         │
│  │          │   │ .schedule│──► SlottedScheduleContext<NodeStrategy> │
│  └──────────┘   └──────────┘                                         │
└─────────────────────────┬────────────────────────────────────────────┘
                          │
                          │ (feasibility / admission queries)
                          │
        ┌─────────────────┼─────────────────┐
        ▼                 ▼                  ▼
   ┌─────────┐     ┌───────────┐     ┌──────────────┐
   │  ADC     │     │   AcI     │     │  Scheduler   │
   │(admiss'n)│     │(controllr)│     │(slot mgmt)   │
   └─────────┘     └───────────┘     └──────────────┘
```
