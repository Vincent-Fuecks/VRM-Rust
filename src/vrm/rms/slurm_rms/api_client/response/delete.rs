use serde::Deserialize;

use super::nodes::{SlurmError, SlurmMeta, SlurmWarning};

#[derive(Debug, Deserialize)]
pub struct SlurmDeleteResponse {
    pub meta: Option<SlurmMeta>,
    pub errors: Option<Vec<SlurmError>>,
    pub warnings: Option<Vec<SlurmWarning>>,
}
