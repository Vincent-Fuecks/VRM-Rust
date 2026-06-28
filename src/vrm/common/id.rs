use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Serialize, Deserialize)]
pub struct Id<T> {
    pub id: String,
    _marker: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new(id: impl Into<String>) -> Self {
        Id { id: id.into(), _marker: PhantomData }
    }

    pub fn compare(&self, other: &Id<T>) -> bool {
        self.id == other.id
    }

    pub fn cast<U>(self) -> Id<U> {
        Id { id: self.id, _marker: PhantomData }
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl<T> From<Id<T>> for String {
    fn from(id_wrapper: Id<T>) -> Self {
        // We can consume the Id<T> and extract the inner String
        id_wrapper.id
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let full_name = std::any::type_name::<T>();
        let clean_name = full_name.split("::").last().unwrap_or(full_name);
        let display_name = clean_name.replace("Tag", "Id");

        write!(f, "{}: {:?}", display_name, self.id)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct ReservationTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct RouterTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy, Serialize, Deserialize)]
pub struct ResourceTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct RmsTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct SlottedScheduleTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct AciTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct AdcTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct ShadowScheduleTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct ClientTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct ComponentTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy)]
pub struct ProbeReservationTag;

// Workflow Domain Tags
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy, Serialize, Deserialize)]
pub struct WorkflowTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy, Serialize, Deserialize)]
pub struct WorkflowNodeTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy, Serialize, Deserialize)]
pub struct DataDependencyTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy, Serialize, Deserialize)]
pub struct SyncDependencyTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy, Serialize, Deserialize)]
pub struct CoAllocationTag;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Copy, Serialize, Deserialize)]
pub struct CoAllocationDependencyTag;

pub type ReservationName = Id<ReservationTag>;
pub type RouterId = Id<RouterTag>;
pub type ResourceName = Id<ResourceTag>;
pub type RmsId = Id<RmsTag>;
pub type SlottedScheduleId = Id<SlottedScheduleTag>;
pub type AciId = Id<AciTag>;
pub type AdcId = Id<AdcTag>;
pub type ShadowScheduleId = Id<ShadowScheduleTag>;
pub type ClientId = Id<ClientTag>;
pub type ComponentId = Id<ComponentTag>;
pub type ProbeReservationId = Id<ProbeReservationTag>;

// Workflow Domain Aliases
pub type WorkflowId = Id<WorkflowTag>;
pub type WorkflowNodeId = Id<WorkflowNodeTag>;
pub type DataDependencyId = Id<DataDependencyTag>;
pub type SyncDependencyId = Id<SyncDependencyTag>;
pub type CoAllocationId = Id<CoAllocationTag>;
pub type CoAllocationDependencyId = Id<CoAllocationDependencyTag>;
