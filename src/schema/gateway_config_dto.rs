use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-RMS gateway configuration.
///
/// Each RMS component can have its own gateway router ID, ingress bandwidth, and egress bandwidth.
/// If `gateway_router_id` is absent, the system falls back to generating `"AcI-Gateway-{component_id}"`.
#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayConfigDto {
    /// Optional custom gateway router ID. Falls back to `"AcI-Gateway-{component_id}"`.
    #[serde(default)]
    pub gateway_router_id: Option<String>,

    /// Maximum ingress bandwidth in Gbps.
    pub ingress_bandwidth_gbps: i64,

    /// Maximum egress bandwidth in Gbps.
    pub egress_bandwidth_gbps: i64,

    /// The switch ID within the RMS topology that the gateway connects to.
    pub gateway_switch_id: String,
}

/// Links two gateway routers across different RMS components.
#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterGatewayLinkDto {
    /// The source gateway router ID.
    pub source_gateway: String,

    /// The target gateway router ID.
    pub target_gateway: String,

    /// Bandwidth capacity of the inter-gateway link in Gbps.
    pub bandwidth_gbps: i64,
}

/// Top-level container for all gateway configuration.
#[derive(Debug, Deserialize, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GatewayConfigSectionDto {
    /// Per-RMS gateway configurations, keyed by component ID.
    #[serde(default)]
    pub gateway_config: HashMap<String, GatewayConfigDto>,

    /// Inter-gateway links connecting gateways across different RMS components.
    #[serde(default)]
    pub inter_gateway_links: Vec<InterGatewayLinkDto>,
}

impl GatewayConfigDto {
    /// Resolves the effective gateway RouterId for this RMS component.
    ///
    /// If `gateway_router_id` is explicitly configured, it is used.
    /// Otherwise, falls back to `"AcI-Gateway-{component_id}"`.
    pub fn resolve_gateway_router_id(&self, component_id: &str) -> String {
        self.gateway_router_id.clone().unwrap_or_else(|| format!("AcI-Gateway-{}", component_id))
    }
}
