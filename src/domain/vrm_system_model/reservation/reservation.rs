use serde::{Deserialize, Serialize};
use std::{any::Any, collections::HashSet, ops::Not};

use crate::{
    domain::vrm_system_model::{
        reservation::{link_reservation::LinkReservation, node_reservation::NodeReservation},
        utils::id::{ClientId, ComponentId, ReservationName, RouterId},
    },
    vrm::workflow::workflow::Workflow,
};

use super::reservation_store::ReservationId;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Reservation {
    Workflow(Workflow),
    Node(NodeReservation),
    Link(LinkReservation),
}

#[derive(Debug)]
pub enum ReservationTyp {
    Workflow,
    Link,
    Node,
}

impl Reservation {
    pub fn new_node(
        base: ReservationBase,
        data_dependencies: HashSet<ReservationId>,
        cwd: Option<String>,
        environment: Option<Vec<String>>,
        task_path: String,
        out_path: Option<String>,
        err_path: Option<String>,
    ) -> Self {
        Self::Node(NodeReservation {
            base,
            data_dependencies,
            current_working_directory: cwd,
            environment: environment,
            task_path: task_path,
            output_path: out_path,
            error_path: err_path,
        })
    }

    pub fn new_link(base: ReservationBase, start: RouterId, end: RouterId) -> Self {
        Self::Link(LinkReservation { base, start_point: Some(start), end_point: Some(end) })
    }

    pub fn get_base_reservation(&self) -> &ReservationBase {
        match self {
            Reservation::Workflow(w) => &w.base,
            Reservation::Node(n) => &n.base,
            Reservation::Link(l) => &l.base,
        }
    }

    pub fn get_base_mut_reservation(&mut self) -> &mut ReservationBase {
        match self {
            Reservation::Workflow(w) => &mut w.base,
            Reservation::Node(n) => &mut n.base,
            Reservation::Link(l) => &mut l.base,
        }
    }

    pub fn state(&self) -> &ReservationState {
        &self.get_base_reservation().state
    }

    /// Returns a mutable reference to the Workflow if this is a Workflow reservation.
    pub fn as_workflow_mut(&mut self) -> Option<&mut Workflow> {
        match self {
            Reservation::Workflow(w) => Some(w),
            _ => None,
        }
    }

    /// Returns  reference to the Workflow if this is a Workflow reservation.
    pub fn as_workflow(&self) -> Option<&Workflow> {
        match self {
            Reservation::Workflow(w) => Some(w),
            _ => None,
        }
    }

    pub fn as_node(&self) -> Option<&NodeReservation> {
        match self {
            Reservation::Node(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_node_mut(&mut self) -> Option<&mut NodeReservation> {
        match self {
            Reservation::Node(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_link(&self) -> Option<&LinkReservation> {
        match self {
            Reservation::Link(l) => Some(l),
            _ => None,
        }
    }
    pub fn as_link_mut(&mut self) -> Option<&mut LinkReservation> {
        match self {
            Reservation::Link(l) => Some(l),
            _ => None,
        }
    }

    pub fn is_link(&self) -> bool {
        match self {
            Reservation::Link(_) => true,
            _ => false,
        }
    }

    pub fn is_node(&self) -> bool {
        match self {
            Reservation::Node(_) => true,
            _ => false,
        }
    }

    pub fn is_workflow(&self) -> bool {
        match self {
            Reservation::Workflow(_) => true,
            _ => false,
        }
    }

    // Only allowed for ProbeReservations
    pub fn set_name(&mut self, new_name: String) {
        if self.get_state() == ReservationState::ProbeAnswer {
            self.get_base_mut_reservation().name = ReservationName::new(new_name);
            return;
        }
        log::error!("set_name not allowed for ReservationState {:?}. Only ProbeAnswer is allowed.", self.get_state());
    }

    pub fn set_frag_delta(&mut self, frag_delta: f64) {
        self.get_base_mut().frag_delta = frag_delta;
    }
}

impl ReservationTrait for Reservation {
    fn get_base(&self) -> &ReservationBase {
        self.get_base_reservation()
    }

    fn get_base_mut(&mut self) -> &mut ReservationBase {
        self.get_base_mut_reservation()
    }

    fn box_clone(&self) -> Box<dyn ReservationTrait> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        match self {
            Reservation::Workflow(workflow) => workflow,
            Reservation::Link(link) => link,
            Reservation::Node(node) => node,
        }
    }

    fn get_type(&self) -> ReservationTyp {
        match self {
            Reservation::Workflow(_) => ReservationTyp::Workflow,
            Reservation::Link(_) => ReservationTyp::Link,
            Reservation::Node(_) => ReservationTyp::Node,
        }
    }
}
pub trait ReservationTrait: std::fmt::Debug + Any + Send + Sync {
    fn get_base(&self) -> &ReservationBase;

    fn get_base_mut(&mut self) -> &mut ReservationBase;

    fn box_clone(&self) -> Box<dyn ReservationTrait>;

    fn as_any(&self) -> &dyn Any;

    fn get_type(&self) -> ReservationTyp;

    fn get_name(&self) -> ReservationName {
        self.get_base().name.clone()
    }

    fn get_assigned_start(&self) -> i64 {
        self.get_base().assigned_start
    }

    fn get_assigned_end(&self) -> i64 {
        self.get_base().assigned_end
    }

    fn is_moldable(&self) -> bool {
        self.get_base().is_moldable
    }

    fn get_reserved_capacity(&self) -> i64 {
        self.get_base().reserved_capacity
    }

    fn get_task_duration(&self) -> i64 {
        self.get_base().task_duration
    }

    fn get_state(&self) -> ReservationState {
        self.get_base().state
    }

    fn get_booking_interval_start(&self) -> i64 {
        self.get_base().booking_interval_start
    }

    fn get_booking_interval_end(&self) -> i64 {
        self.get_base().booking_interval_end
    }

    fn get_moldable_work(&self) -> i64 {
        self.get_base().moldable_work
    }

    fn get_client_id(&self) -> ClientId {
        self.get_base().client_id.clone()
    }

    fn get_handler_id(&self) -> Option<ComponentId> {
        self.get_base().handler_id.clone()
    }

    fn get_reservation_proceeding(&self) -> ReservationProceeding {
        self.get_base().request_proceeding
    }

    fn get_arrival_time(&self) -> i64 {
        self.get_base().arrival_time
    }

    fn set_assigned_end(&mut self, time: i64) {
        self.get_base_mut().assigned_end = time;
    }

    fn set_assigned_start(&mut self, time: i64) {
        self.get_base_mut().assigned_start = time;
    }

    fn set_state(&mut self, reservation_state: ReservationState) {
        self.get_base_mut().state = reservation_state;
    }

    /**
     * Changes the job duration (VRM time in s) and change also the capacity/duration quotient
     * for moldable reservations. To adjust the duration, while keeping the
     * quotient, use #adjustJobDuration.
     *
     * @param jobDuration
     *            the job duration in s (VRM time)
     * @see #adjustJobDuration
     * @see #adjustCapacity
     * @see #setReservedCapacity(int)
     */
    fn set_task_duration(&mut self, duration: i64) {
        self.get_base_mut().task_duration = duration;
        self.get_base_mut().moldable_work = self.get_base().reserved_capacity * duration
    }

    /**
     * Changes the capacity value and change also the capacity/duration quotient
     * for moldable reservations.
     *
     * The capacity is measured in a unit according to the job type e.g. number
     * of CPUs for {@link NodeReservation} or kBit/s Bandwidth for
     * {@link LinkReservation}.
     *
     * @param reservedCapacity
     *            the reservedCapacity to set
     * @see Reservation#adjustCapacity(int)
     */
    fn set_reserved_capacity(&mut self, reserved_capacity: i64) {
        self.get_base_mut().reserved_capacity = reserved_capacity;
        self.get_base_mut().moldable_work = reserved_capacity * self.get_task_duration()
    }

    fn set_booking_interval_start(&mut self, start_time: i64) {
        self.get_base_mut().booking_interval_start = start_time;
    }

    fn set_booking_interval_end(&mut self, end_time: i64) {
        self.get_base_mut().booking_interval_end = end_time;
    }

    fn set_frag_delta(&mut self, frag_delta: f64) {
        self.get_base_mut().frag_delta = frag_delta;
    }

    fn set_is_moldable(&mut self, is_moldable: bool) {
        self.get_base_mut().is_moldable = is_moldable;
    }

    fn adjust_capacity(&mut self, capacity: i64) {
        if capacity != self.get_base().reserved_capacity {
            if self.is_moldable().not() {
                log::warn!("adjustCapacity for non moldable job {}", self.get_base().get_name(),);
            }

            if capacity == 0 {
                self.set_task_duration(self.get_moldable_work());
                self.set_reserved_capacity(1);
            } else {
                self.set_task_duration(self.get_moldable_work() / capacity);
                self.set_reserved_capacity(capacity);
            }

            if self.get_task_duration() <= 0 {
                self.set_task_duration(1);
            }
        }
    }
    fn adjust_task_duration(&mut self, duration: i64) {
        self.get_base_mut().adjust_task_duration(duration);
    }
}

impl Clone for Box<dyn ReservationTrait> {
    fn clone(&self) -> Box<dyn ReservationTrait> {
        self.box_clone()
    }
}

/// Defines the lifecycle state of a job reservation within the system.
///
/// This state tracks the progress of the reservation from initial request
/// through processing, commitment, and eventual completion or failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ReservationState {
    /// The last request of the reservation was explicitly denied or failed.
    Rejected,

    /// The reservation has been successfully cancelled and removed from the system.
    Deleted,

    /// The reservation is newly created and has not yet been submitted to any processor.
    Open,

    /// The state represents a successful response to a probing (availability) request.
    /// Success includes also an empty ProbeAnswer, but only if the request was feasible resource wise and no error in the request process occurred.
    ProbeAnswer,

    /// The state represents the product of a successful probe request of a specific AcI.
    /// During a probe request, all potential time slots where a reservation on an AcI can run are collected.
    /// A ProbeReservation represents such a potential reservation within a valid time window
    /// (these reservations are based on the original Reservation,
    /// which is stored in the ReservationStore in state ProbeAnswer).
    /// These reservations are only tracked by the AcI system and are not part of the ReservationStore.
    /// The only valid state transitions are ReserveProbeReservation, Rejected, and Deleted.
    /// If the ProbeReservation is promoted to ReserveProbeReservation, it replaces the original ProbeAnswer
    /// Reservation, and all other ProbeReservations for the original ProbeAnswer Reservation become invalid.
    ProbeReservation,

    /// The state signals the listening AcI, that the ProbeAnswer was successful and the Reservation should be
    /// Reserved -> AcI performs reserve for Reservation
    ReserveProbeReservation,

    /// The state represents a successful response to a resource reservation request.
    ReserveAnswer,

    /// The reservation has been confirmed and was committed to the local RMS.
    Committed,

    /// The execution phase of the job linked to this reservation has been finished successfully.
    Finished,

    // Is a job, that is not from the VRM system, but from the local RMS.
    External,
}

impl ReservationState {
    pub fn from_slurm_task_state(str_state: &str) -> Result<Self, String> {
        match str_state {
            // "Active" states: The job is either queued (SlurmTaskState::Pending) or actively holding resources.
            // Hold the allocated resources except task in state PENDING which still waits to start.
            "PENDING" | "RUNNING" | "SUSPENDED" | "COMPLETING" => Ok(ReservationState::Committed),

            // "Success" state: Job finished normally and released the resources.
            // Released the allocated resources
            "COMPLETED" => Ok(ReservationState::Finished),

            // "Cancelled" state: Explicit user or admin deletion.
            // Released the allocated resources
            "CANCELLED" => Ok(ReservationState::Deleted),

            // "Failure" states: The job was rejected by the system or failed during execution.
            // Released the allocated resources
            "FAILED" | "TIMEOUT" | "NODE_FAIL" | "PREEMPTED" | "BOOT_FAIL" | "DEADLINE" | "OUT_OF_MEMORY" => Ok(ReservationState::Rejected),

            _ => Err(format!("Unknown Slurm state: {}", str_state)),
        }
    }

    pub fn is_reserve_request_valid(&self) -> bool {
        matches!(self, Self::Open | Self::ReserveProbeReservation | Self::ProbeReservation)
    }

    pub fn is_reservation_at_cycle_end(&self, res_proceeding_state: ReservationProceeding) -> bool {
        match res_proceeding_state {
            ReservationProceeding::Commit => {
                matches!(self, Self::Finished | Self::Rejected | Self::Deleted)
            }
            ReservationProceeding::Delete => {
                matches!(self, Self::Rejected | Self::Deleted)
            }
            ReservationProceeding::Ignore => false,
            ReservationProceeding::Probe => {
                matches!(self, Self::Finished | Self::Rejected | Self::Deleted | Self::ProbeAnswer)
            }
            ReservationProceeding::Reserve => {
                matches!(self, Self::Finished | Self::Rejected | Self::Deleted | Self::ReserveAnswer)
            }
        }
    }
}

/// Defines the set of primary actions (proceedings) that can be requested for a reservation.
///
/// This determines the lifecycle stage a reservation is intended to reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReservationProceeding {
    /// Executes only the initial resource availability **probe** request to check feasibility.
    /// No resources are formally reserved.
    Probe,

    /// Executes a **reserve** request to temporarily hold resources. The reservation
    /// remains active but requires a subsequent `Commit` or `Delete` action.
    Reserve,

    /// Executes the full sequence: **reserve** followed immediately by a **commit** to finalize
    /// the resource allocation.
    Commit,

    /// Deletes the reservation in the system with the corresponding id.
    Delete,

    /// Vrm system will not interact with this task, because this is an external task form a local rms.
    Ignore,
}

/// The fundamental structure holding common data for any resource reservation system.
///
/// This base provides essential metadata, time information, and capacity requirements,
/// regardless of whether the reservation targets a computational node, network link or workflow.
///
/// It is also possible, that more than one object representing the same reservation.
/// Each component has to take care to sync the state of all these objects,
/// e.g., between the object in the reservation database and the return value of some request.
///
/// To support this synchronization each reservation has to carry a unique id within the VRM setup.
/// The component initially creating the reservation has to take care to generate only unique names.
/// Objects representing the same reservation, but in different states
/// or different configurations have to carry the very same id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservationBase {
    /// A globally unique identifier for this reservation across all distributed components.
    pub name: ReservationName,

    /// Is the Id of the client, how submitted the reservation into the VRM system.
    pub client_id: ClientId,

    /// Contains the Id of the components, how is handling the reservation currently (Adc or AcI).
    pub handler_id: Option<ComponentId>,

    /// The current **lifecycle state** of the reservation (e.g., `Open`, `Committed`, `Finished`).
    pub state: ReservationState,

    /// The **requested action** for the reservation (e.g., `Probe`, `Reserve`, `Commit`).
    pub request_proceeding: ReservationProceeding,

    // Time windows in s
    /// The time at which the reservation request **arrived** in the system.
    pub arrival_time: i64,

    /// The earliest time the resource is **requested to start** the booking interval.
    pub booking_interval_start: i64,

    /// The latest time the resource is **requested to end** the booking interval.
    pub booking_interval_end: i64,

    /// The precise time the resource was **formally assigned to start** the task. (0 if not set).
    pub assigned_start: i64,

    /// The precise time the resource was **formally assigned to end** the task. (0 if not set).
    pub assigned_end: i64,

    // Resource properties
    /// The requested and reserved **duration** of the task (in seconds).
    pub task_duration: i64, // 'duration' in NodeReservationDto

    /// The total **amount of resource capacity** requested and reserved of this task
    /// Unit is according to the Task:
    /// NodeReservation: Number of CPUs
    /// LinkReservation: Bandwidth in MB's
    pub reserved_capacity: i64,

    // Flag indicating if the task's duration and capacity are **flexible** (moldable/ can be adjusted).
    pub is_moldable: bool,

    /// Internal field: The total required work, calculated as `reserved_capacity` * `task_duration`.
    /// This value remains constant for non-moldable jobs.
    pub moldable_work: i64,

    /// Currently not used by the VRM.
    /// Represents the change in fragmentation resulting from a new reservation.
    ///
    /// # Example
    /// If the current fragmentation degree of the schedule is 0.5 and a new reservation
    /// increases it to 0.6, the `frag_delta` is the difference indicating that the
    /// fragmentation has worsened. This value is intended for use by a `WorkflowScheduler`
    /// or other schedule optimization criteria.
    pub frag_delta: f64,
}

impl ReservationBase {
    pub fn is_moldable(&self) -> bool {
        self.is_moldable
    }

    pub fn get_reserved_capacity(&self) -> i64 {
        self.reserved_capacity
    }

    pub fn get_assigned_start(&self) -> i64 {
        self.assigned_start
    }

    pub fn get_assigned_end(&self) -> i64 {
        self.assigned_end
    }

    pub fn get_name(&self) -> ReservationName {
        self.name.clone()
    }

    pub fn get_task_duration(&self) -> i64 {
        self.task_duration
    }

    pub fn get_state(&self) -> ReservationState {
        self.state
    }

    pub fn get_booking_interval_start(&self) -> i64 {
        self.booking_interval_start
    }

    pub fn get_booking_interval_end(&self) -> i64 {
        self.booking_interval_end
    }

    pub fn get_moldable_work(&self) -> i64 {
        self.moldable_work
    }

    pub fn get_client_id(&self) -> ClientId {
        self.client_id.clone()
    }

    pub fn get_handler_id(&self) -> Option<ComponentId> {
        self.handler_id.clone()
    }

    pub fn get_reservation_proceeding(&self) -> ReservationProceeding {
        self.request_proceeding
    }

    pub fn get_arrival_time(&self) -> i64 {
        self.arrival_time
    }

    pub fn set_assigned_end(&mut self, time: i64) {
        self.assigned_end = time;
    }

    pub fn set_assigned_start(&mut self, time: i64) {
        self.assigned_start = time;
    }

    pub fn set_state(&mut self, reservation_state: ReservationState) {
        self.state = reservation_state;
    }

    pub fn set_task_duration(&mut self, duration: i64) {
        self.task_duration = duration;
    }

    pub fn set_reserved_capacity(&mut self, reserved_capacity: i64) {
        self.reserved_capacity = reserved_capacity;
    }

    pub fn set_booking_interval_start(&mut self, start_time: i64) {
        self.booking_interval_start = start_time;
    }

    pub fn set_booking_interval_end(&mut self, end_time: i64) {
        self.booking_interval_end = end_time;
    }

    pub fn set_frag_delta(&mut self, frag_delta: f64) {
        self.frag_delta = frag_delta;
    }

    /**
     * Adjust the job duration and requested capacity for moldable reservations.
     * This means the method changes the duration and capacity such that the
     * quotient of both stays constant. For inherit changes of the job size use
     * {@link #setReservedCapacity(int)}.
     *
     * The capacity is measured in a unit according to the job type e.g. number
     * of CPUs for {@link NodeReservation} or kBit/s Bandwidth for
     * {@link LinkReservation}.
     *
     * This method can only be called if {@link #isMoldable()} return
     * <code>true</code>.
     *
     * @param capacity
     *            the new capacity
     * @see #setReservedCapacity
     * @see #adjustJobDuration(int)
     */
    pub fn adjust_capacity(&mut self, capacity: i64) {
        if capacity != self.reserved_capacity {
            if self.is_moldable().not() {
                log::warn!("adjustCapacity for non moldable job {}", self.get_name(),);
            }

            if capacity == 0 {
                self.set_task_duration(self.get_moldable_work());
                self.set_reserved_capacity(1);
            } else {
                self.set_task_duration(self.get_moldable_work() / capacity);
                self.set_reserved_capacity(capacity);
            }

            if self.get_task_duration() <= 0 {
                self.set_task_duration(1);
            }
        }
    }

    /**
     * Adjust the job duration and requested capacity for moldable reservations.
     * This means the method changes the duration and capacity such that the
     * quotient of both stays constant. For inherit changes of the job size use
     * {@link #setJobDuration(int)}.
     *
     * This method can only be called if {@link #isMoldable()} return
     * <code>true</code>.
     *
     * @param duration
     *            the job duration in s (VRM time)
     * @see #setJobDuration
     * @see #adjustCapacity(int)
     */
    pub fn adjust_task_duration(&mut self, mut duration: i64) {
        if duration != self.get_task_duration() {
            if !self.is_moldable() {
                log::error!(
                    "ErrorAdjustedNonMoldableTaskDuration: Adjusted Task duration from {} to {} of reservation with name {}",
                    self.get_task_duration(),
                    duration,
                    self.get_name()
                )
            }

            if duration <= 0 {
                duration = 1;
            }

            self.reserved_capacity = self.get_moldable_work() / duration;

            if self.reserved_capacity <= 0 {
                self.set_reserved_capacity(1);
            }

            self.set_task_duration(duration);
        }
    }
}
