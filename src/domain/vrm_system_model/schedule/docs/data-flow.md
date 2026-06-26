# Schedule Component — Data Flow

**Last Updated:** 2025-01-XX

---

## 1. Reservation Lifecycle

### 1.1 Probe Flow

```
ReservationId (request)
    │
    ▼
┌─────────────────────────────────────────────┐
│  Schedule::probe(id)                         │
│  (schedule_base.rs)                          │
│                                             │
│  ┌── Early Stop: negative capacity? ──► Rejected
│  │                                             
│  ├── SlottedScheduleContext::update()          
│  │   (advance window to current time)          
│  │                                             
│  ├── calculate_schedule(id)                    
│  │   │                                         
│  │   ├── Determine search bounds                
│  │   │   (booking_interval_start..end)          
│  │   │                                         
│  │   ├── For each feasible start slot:          
│  │   │   └── try_fit_reservation(id, start, end)
│  │   │       │                                 
│  │   │       ├── For each slot in range:        
│  │   │       │   └── S::adjust_requirement...() 
│  │   │       │       (Node or Link strategy)    
│  │   │       │                                 
│  │   │       └── If feasible → add to candidates
│  │   │                                         
│  │   └── Return ProbeReservations               
│  │                                             
│  └── If is_frag_needed:                        
│      For each candidate:                       
│      ├── reserve_without_check() (temp)        
│      ├── get_system_fragmentation()            
│      ├── compute frag_delta                    
│      └── delete_reservation() (undo temp)      
│                                             
│  Output: ProbeReservations                      
└─────────────────────────────────────────────┘
    │
    ▼
ProbeReservations { original_id, candidates[] }
```

### 1.2 Probe-Best Flow

```
ReservationId + ProbeReservationComparator
    │
    ▼
┌─────────────────────────────────────────────┐
│  Schedule::probe_best(id, comparator)        │
│                                             │
│  ├── Early Stop: negative capacity? ──► Rej.
│  ├── probe(id) → ProbeReservations          
│  ├── create_new_probe_reservation_with_best()
│  └── Return single-candidate ProbeReservations
│                                             │
│  Output: ProbeReservations (1 candidate)     
└─────────────────────────────────────────────┘
```

### 1.3 Reserve Flow

```
ReservationId (probe-approved candidate)
    │
    ▼
┌─────────────────────────────────────────────┐
│  Schedule::reserve(id)                       │
│                                             │
│  ├── Early Stop: negative capacity? ──► Rej.
│  ├── update()                                
│  ├── calculate_schedule(id)                  
│  ├── only_prompt_best(id, EST comparator)    
│  │   │                                       
│  │   ├── If success → reserve_without_check()
│  │   │   │                                   
│  │   │   └── For each affected slot:         
│  │   │       └── S::insert_reservation_into_slot()
│  │   │           (Node: slot.insert_reservation)
│  │   │           (Link: book path, book each link)
│  │   │                                       
│  │   └── Return Some(id)                     
│  │                                           
│  └── If failure → Rejected, return None      
│                                             │
│  Output: Option<ReservationId>               
│  Some(id) = committed, None = rejected       
└─────────────────────────────────────────────┘
```

### 1.4 Reserve-Without-Check Flow (Internal)

```
ReservationId (pre-validated)
    │
    ▼
┌─────────────────────────────────────────────┐
│  Schedule::reserve_without_check(id)         │
│                                             │
│  ├── Early Stop: negative capacity? ──► Rej.
│  ├── Compute start/end slot indices          
│  ├── For each slot:                          
│  │   └── S::insert_reservation_into_slot()   
│  ├── Insert into active_reservations set     
│  └── Update state → ReserveAnswer            
└─────────────────────────────────────────────┘
```

### 1.5 Deletion Flow

```
ReservationId
    │
    ▼
┌─────────────────────────────────────────────┐
│  Schedule::delete_reservation(id)            │
│                                             │
│  ├── is_reservation_valid_for_deletion()     │
│  │   Check: is reservation active?           │
│  │   If not → Rejected, return                │
│  │                                            │
│  ├── Check: is reservation already finished? │
│  │   (assigned_end <= current_time) → return  │
│  │                                            │
│  ├── Remove from active_reservations set      │
│  ├── For each occupied slot:                 │
│  │   └── slot.delete_reservation(id, cap)    │
│  ├── Invalidate frag cache                    │
│  └── S::on_delete_reservation()              │
│      (Link: clean up reserved_paths entry)    │
└─────────────────────────────────────────────┘
```

---

## 2. Window Update Flow

```
SlottedScheduleContext::update()
    │
    ▼
┌─────────────────────────────────────────────┐
│  Called: on every probe/reserve/delete        │
│                                             │
│  1. Get current_time from GlobalClock         │
│  2. Compute new_start_slot_index              │
│                                             │
│  3. If window advanced → invalidate frag cache│
│                                             │
│  4. Collect IDs to remove:                    │
│     For each slot before new_start:           │
│       Check each reservation's last slot      │
│       If last slot == clean_index → mark for  │
│       removal from active_reservations        │
│                                             │
│  5. Move historical loads to LoadBuffer:      │
│     For each slot before new_start:           │
│       slot.load → load_buffer.add(load, idx)  │
│       slot.reset()                            │
│                                             │
│  6. Update window pointers:                   │
│     start_slot_index = new_start              │
│     end_slot_index = new_start + num_slots - 1│
│     scheduling_window_start_time = ...        │
│     scheduling_window_end_time = ...          │
└─────────────────────────────────────────────┘
```

---

## 3. Fragmentation Data Flow

### 3.1 Quadratic Mean Method

```
Input: start_slot_index, end_slot_index
    │
    ▼
┌─────────────────────────────────────────────┐
│  get_fragmentation_quadratic_mean()          │
│                                             │
│  Initialize per-capacity tracking arrays:    │
│  quad_sum_per_free_block[0..=max_capacity]   │
│  sum_per_free_block[0..=max_capacity]        │
│  current_free_block_len[0..=max_capacity]    │
│                                             │
│  Phase 1: add_block_which_end_in_range()     │
│  For each slot in range:                     │
│    free = capacity - slot.load               │
│    For cap=1..=free: block_len[cap]++        │
│    For cap=free+1..=max:                     │
│      if block_len[cap] > 0:                  │
│        quad_sum += block_len²                │
│        sum += block_len                      │
│        block_len = 0                         │
│                                             │
│  Phase 2: add_block_which_are_cut_by_end()   │
│  Flush remaining blocks at range end          │
│                                             │
│  Phase 3: calculate_avg_fragmentation()      │
│  For each capacity level:                    │
│    if sum>0: frag = quad_sum / sum^power     │
│  return 1 - avg(frag_values)                 │
│                                             │
│  Output: f64 (0.0 = best, 1.0 = worst)      │
└─────────────────────────────────────────────┘
```

### 3.2 Resubmit Method

```
Input: start_slot_index, end_slot_index
    │
    ▼
┌─────────────────────────────────────────────┐
│  get_fragmentation_resubmit()                │
│                                             │
│  1. Calculate free_capacity_in_range         │
│  2. If no active reservations → return 0.0   │
│                                             │
│  3. Clone the schedule (expensive!)          │
│                                             │
│  4. While remaining_capacity > 0:            │
│     ├── Pick random active reservation       │
│     │   (that overlaps the range)            │
│     ├── Try test_schedule.reserve(id)        │
│     ├── If rejected:                         │
│     │   remaining -= reserved_capacity       │
│     │   rejected += reserved * duration      │
│     └── If success:                          │
│         remaining -= reserved_capacity       │
│                                             │
│  5. Return rejected_capacity / total_free    │
│                                             │
│  Output: f64 (0.0 = best, 1.0 = worst)      │
└─────────────────────────────────────────────┘
```

---

## 4. Capacity Update Flow

```
Schedule::update_capacity(capacity: usize)
    │
    ▼
┌─────────────────────────────────────────────┐
│  SlottedScheduleContext::update_capacity()   │
│                                             │
│  For each slot:                              │
│  ├── If slot.load < new_capacity:            │
│  │   slot.capacity = new_capacity            │
│  │   (no eviction needed)                    │
│  │                                           │
│  └── If slot.load >= new_capacity:           │
│      ├── Clone reservation_ids set           │
│      ├── For each reservation in set:        │
│      │   ├── slot.delete_reservation(id, cap)│
│      │   └── If slot.load < new_capacity:    │
│      │       slot.capacity = new_capacity    │
│      │       break (stop eviction)           │
│      └── (reservations may be partially kept)│
└─────────────────────────────────────────────┘
```

---

## 5. Link Strategy Data Flow (Network Path Selection)

### 5.1 Capacity Check (Probe Phase)

```
LinkStrategy::adjust_requirement_to_slot_capacity()
    │
    ▼
┌─────────────────────────────────────────────┐
│  Input: slot_index, requirement, res_id       │
│                                             │
│  1. Get source/target from reservation_store  │
│  2. Look up cached paths from topology:       │
│     path_cache[(source, target)]              │
│                                             │
│  3. For each cached path:                    │
│     ├── path_available = max_bandwidth        │
│     ├── For each link in path:               │
│     │   path_available = min(                 │
│     │     path_available,                     │
│     │     NodeStrategy::adjust_requirement(   │
│       link_schedule, slot, path_avail)        │
│     │   )                                     │
│     ├── If path_available == max_bandwidth:   │
│     │   return max_bandwidth (early exit)    │
│     └── Track max across all paths            │
│                                             │
│  4. Return best available capacity found      │
└─────────────────────────────────────────────┘
```

### 5.2 Reservation Insertion (Commit Phase)

```
LinkStrategy::insert_reservation_into_slot()
    │
    ▼
┌─────────────────────────────────────────────┐
│  Input: requirement, slot_index, res_id       │
│                                             │
│  1. Get source/target from reservation_store  │
│  2. Look up cached paths from topology        │
│                                             │
│  3. For each cached path:                    │
│     ├── Check ALL links: is path free?       │
│     │   (adjust_requirement == requirement)  │
│     ├── If path is free:                     │
│     │   ├── For each link:                   │
│     │   │   NodeStrategy::insert_reservation │
│     │   │   (link_schedule, req, slot)       │
│     │   ├── Store path in reserved_paths     │
│     │   │   [res_id][slot_index] = path       │
│     │   ├── Insert into this slot            │
│     │   └── return (success)                 │
│     │                                         │
│  4. If no path found: log error              │
└─────────────────────────────────────────────┘
```

---

## 6. Load Metric Data Flow

### 6.1 Range-based Load Metric

```
Schedule::get_load_metric(start_time, end_time)
    │
    ▼
┌─────────────────────────────────────────────┐
│  NodeStrategy::get_load_metric()             │
│                                             │
│  1. Convert times to slot indices            │
│  2. Clamp to scheduling window               │
│  3. Sum slot.load over [start_slot..end_slot] │
│  4. avg_reserved = sum / num_slots           │
│  5. utilization = avg / capacity              │
│                                             │
│  Output: LoadMetric {                        │
│    start_time, end_time,                     │
│    avg_reserved_capacity,                    │
│    possible_capacity,                        │
│    utilization                               │
│  }                                           │
└─────────────────────────────────────────────┘
```

### 6.2 Simulation Load Metric

```
Schedule::get_simulation_load_metric()
    │
    ▼
┌─────────────────────────────────────────────┐
│  NodeStrategy::get_simulation_load_metric()  │
│                                             │
│  1. Get first/last global load indices       │
│     (from GlobalLoadContext)                  │
│  2. Apply SLOTS_TO_DROP_ON_START/END         │
│     (warm-up/cool-down trimming)             │
│  3. Delegate to LoadBuffer:                  │
│     get_effective_overall_load()             │
│     │                                        │
│     ├── Fill tail with zeros if needed       │
│     ├── sum_reserved - tail_buffer sum       │
│     ├── Compute avg over effective range     │
│     └── Return LoadMetric                    │
└─────────────────────────────────────────────┘
```

---

## 7. Data Dependencies Between Components

```
                     ┌─────────────┐
                     │ GlobalClock │
                     └──────┬──────┘
                            │ get_system_time_s()
                            ▼
┌──────────────────────────────────────────────┐
│               SlottedScheduleContext         │
├──────────────────────────────────────────────┤
│  Uses:                                       │
│  ├── ReservationStore ──── reads/writes      │
│  │   reservation state, capacity, timing     │
│  │                                            │
│  ├── Reservations ──── active set tracking   │
│  ├── LoadBuffer ──── historical load data    │
│  └── GlobalLoadContext ──── sync index bounds│
└──────────────────────────────────────────────┘
         │                          │
         │ adjust_requirement()     │ insert_reservation_into_slot()
         ▼                          ▼
┌──────────────────────┐  ┌──────────────────────┐
│    NodeStrategy      │  │    LinkStrategy      │
│ (single resource)    │  │ (network bandwidth)   │
└──────────────────────┘  └──────────┬───────────┘
                                     │
                          ┌──────────┴───────────┐
                          │   NetworkTopology     │
                          │                      │
                          │  path_cache ──────►  │
                          │  ResourceStore       │
                          │  (link_schedules)    │
                          └──────────────────────┘
```
