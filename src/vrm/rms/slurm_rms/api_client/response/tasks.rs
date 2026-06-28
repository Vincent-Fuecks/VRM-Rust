use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct SlurmWrapped<T>(pub T);

impl<'de, T: Deserialize<'de> + Default> Deserialize<'de> for SlurmWrapped<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawValue<U> {
            value: Option<U>,
            number: Option<U>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper<U> {
            Direct(U),
            Wrapped(RawValue<U>),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Direct(v) => Ok(SlurmWrapped(v)),
            Helper::Wrapped(w) => {
                let val = w.value.or(w.number).unwrap_or_default();
                Ok(SlurmWrapped(val))
            }
        }
    }
}

pub trait SlurmOptionExt<T> {
    /// Unwraps the inner value or returns a provided default
    fn val_or(&self, default: T) -> T;

    /// Unwraps the inner value or returns the standard default for the type (e.g., 0 for integers)
    fn val_or_default(&self) -> T
    where
        T: Default;
}

impl<T: Clone> SlurmOptionExt<T> for Option<SlurmWrapped<T>> {
    fn val_or(&self, default: T) -> T {
        self.as_ref().map(|w| w.0.clone()).unwrap_or(default)
    }

    fn val_or_default(&self) -> T
    where
        T: Default,
    {
        self.as_ref().map(|w| w.0.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Deserialize)]
pub struct SlurmTaskResponse {
    pub meta: Option<SlurmMeta>,
    pub errors: Option<Vec<SlurmError>>,
    pub warnings: Option<Vec<SlurmWarning>>,
    pub jobs: Vec<SlurmTask>,
}

#[derive(Debug, Deserialize)]
pub struct SlurmMeta {
    pub plugin: Option<SlurmPlugin>,
    pub slurm: Option<SlurmVersionInfo>,
}

#[derive(Debug, Deserialize)]
pub struct SlurmPlugin {
    pub r#type: String,
    pub name: String,
    pub data_parser: String,
}

#[derive(Debug, Deserialize)]
pub struct SlurmVersionInfo {
    pub release: String,
}

#[derive(Debug, Deserialize)]
pub struct SlurmError {
    pub error: String,
    pub error_number: i32,
    pub description: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SlurmWarning {
    pub description: String,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SlurmTask {
    pub job_id: u32,
    pub name: Option<SlurmWrapped<String>>,
    pub job_state: Option<Vec<String>>,
    pub user_name: Option<SlurmWrapped<String>>,
    pub job_resources: Option<SlurmJobResources>,
    pub time: Option<SlurmTime>,
    pub command: Option<SlurmWrapped<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SlurmJobResources {
    pub nodes: Option<SlurmWrapped<String>>,
    pub allocated_cpus: Option<SlurmWrapped<i64>>,
}

#[derive(Debug, Deserialize)]
pub struct SlurmTime {
    /// Number of seconds the job has been running
    pub elapsed: Option<SlurmWrapped<u64>>,

    /// Time limit in minutes (Slurm default) or seconds depending on config
    pub limit: Option<SlurmWrapped<u64>>,

    /// Unix timestamp of actual or expected start
    pub start: Option<SlurmWrapped<u64>>,

    /// Unix timestamp of expected end (start + limit)
    pub end: Option<SlurmWrapped<u64>>,

    /// Unix timestamp of job submission
    pub submission: Option<SlurmWrapped<u64>>,

    /// Unix timestamp of when the job became eligible for scheduling
    pub eligible: Option<SlurmWrapped<u64>>,
}
