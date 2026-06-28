use anyhow::{Context, Ok, Result};
use reqwest::{Client, header};

use crate::schema::rms_dto::SlurmConfigDto;

use super::payload::task_properties::TaskSubmission;
use super::response::delete::SlurmDeleteResponse;
use super::response::nodes::SlurmNodesResponse;
use super::response::task_submit::TaskSubmitResponse;
use super::response::tasks::SlurmTaskResponse;
use super::slurm_endpoint::SlurmEndpoint;
use super::slurm_rest_api_trait::SlurmRestApi;

#[derive(Debug, Clone)]
pub struct SlurmConfig {
    pub base_url: String,
    pub version: String,
    pub user_name: String,
    pub jwt_token: String,
}

#[derive(Debug, Clone)]
pub struct SlurmRestApiClient {
    pub client: Client,
    config: SlurmConfig,
}

impl SlurmRestApiClient {
    pub fn new(slurm_config_dto: SlurmConfigDto) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert("X-SLURM-USER-NAME", header::HeaderValue::from_str(&slurm_config_dto.user_name)?);
        headers.insert("X-SLURM-USER-TOKEN", header::HeaderValue::from_str(&slurm_config_dto.jwt_token)?);
        headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));

        let client = Client::builder().default_headers(headers).build()?;

        let config = SlurmConfig {
            base_url: slurm_config_dto.base_url,
            version: slurm_config_dto.version,
            user_name: slurm_config_dto.user_name,
            jwt_token: slurm_config_dto.jwt_token,
        };

        Ok(Self { client, config })
    }

    /// Helper to build the full URL: base + version + endpoint
    pub fn url(&self, endpoint: &str) -> String {
        format!("{}/slurm/{}{}", self.config.base_url, self.config.version, endpoint)
    }
}

/// Implementation of the [`SlurmRestApi`] trait for the [`SlurmRestApiClient`].
///
/// This implementation handles the low-level HTTP communication, URL construction
/// via [`SlurmEndpoint`], and error handling specific to Slurm's REST response schemas.
#[async_trait::async_trait]
impl SlurmRestApi for SlurmRestApiClient {
    async fn get_nodes(&self) -> Result<SlurmNodesResponse> {
        let res = self.client.get(self.url(&SlurmEndpoint::Nodes.path())).send().await.context("Failed to send node request to Slurm")?;

        return res.error_for_status()?.json().await.context("Failed to parse SlurmNodesResponse");
    }

    async fn get_tasks(&self) -> Result<SlurmTaskResponse> {
        let res = self.client.get(self.url(&SlurmEndpoint::Jobs.path())).send().await.context("Failed to send jobs request to Slurm")?;

        return res.error_for_status()?.json().await.context("Failed to parse SlurmTaskResponse");
    }

    async fn is_rms_alive(&self) -> Result<bool> {
        let res = self.client.get(self.url(&SlurmEndpoint::Ping.path())).send().await.context("Failed to reach Slurm REST API for ping")?;

        return Ok(res.status().is_success());
    }

    async fn commit(&self, payload: TaskSubmission) -> Result<u32> {
        let res = self
            .client
            .post(self.url(&SlurmEndpoint::JobSubmit.path()))
            .json(&payload)
            .send()
            .await
            .context("Failed to connect to Slurm REST API during submission.")?;

        if !res.status().is_success() {
            let status = res.status();
            let err_body = res.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
            anyhow::bail!("Slurm job submission failed [Status {}]: {}", status, err_body);
        }

        let response_data: TaskSubmitResponse = res.json().await.context("Failed to parse submission response")?;

        // Return the job id for meta-scheduler logic
        return response_data
            .job_id
            .ok_or_else(|| anyhow::anyhow!("Slurm accepted the job but didn't return an Id: Error context: {:?}", response_data.error));
    }

    async fn delete(&self, task_id: u32) -> Result<bool> {
        let path = format!("{}/{}", SlurmEndpoint::Job.path(), task_id);

        let res = self.client.delete(self.url(&path)).send().await.context(format!("Failed to send delete request for task {}", task_id))?;

        let response_data: SlurmDeleteResponse = res.json().await.context("Failed to parse delete response")?;

        if let Some(errors) = response_data.errors {
            if !errors.is_empty() {
                anyhow::bail!("Slurm encountered an error in the deletion process: {:?}", errors);
            }
        }

        return Ok(true);
    }
}
