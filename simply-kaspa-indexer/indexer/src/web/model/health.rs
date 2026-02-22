use kaspa_rpc_core::GetServerInfoResponse;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use std::fmt;
use utoipa::ToSchema;

#[skip_serializing_none]
#[derive(ToSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub status: HealthStatus,
    #[schema(example = "1738706345528")]
    pub last_updated: u64,
    pub indexer: HealthIndexer,
    pub kaspad: HealthKaspad,
}

#[derive(ToSchema, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    UP,
    WARN,
    DOWN,
}

#[skip_serializing_none]
#[derive(ToSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIndexer {
    pub status: HealthStatus,
    pub info: HealthIndexerInfo,
    pub details: Option<Vec<HealthIndexerDetails>>,
}

#[skip_serializing_none]
#[derive(ToSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIndexerInfo {
    pub name: String,
    pub version: String,
    pub commit_id: String,
    pub uptime: Option<String>,
}

#[skip_serializing_none]
#[derive(ToSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIndexerDetails {
    pub name: String,
    pub status: HealthStatus,
    pub reason: String,
}

#[skip_serializing_none]
#[derive(ToSchema, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthKaspad {
    pub status: HealthStatus,
    #[schema(example = "true")]
    pub is_synced: Option<bool>,
    #[schema(example = "0.16.0")]
    pub server_version: Option<String>,
    #[schema(example = "mainnet")]
    pub network_id: Option<String>,
    #[schema(example = "102253066")]
    pub virtual_daa_score: Option<u64>,
    pub error: Option<String>,
}

impl From<(HealthStatus, String)> for HealthKaspad {
    fn from(status_error: (HealthStatus, String)) -> Self {
        let (status, error) = status_error;
        HealthKaspad { status, server_version: None, network_id: None, virtual_daa_score: None, is_synced: None, error: Some(error) }
    }
}

impl From<GetServerInfoResponse> for HealthKaspad {
    fn from(get_server_info_response: GetServerInfoResponse) -> Self {
        let is_synced = get_server_info_response.is_synced;
        HealthKaspad {
            status: if is_synced { HealthStatus::UP } else { HealthStatus::DOWN },
            is_synced: Some(is_synced),
            server_version: Some(get_server_info_response.server_version),
            network_id: Some(get_server_info_response.network_id.to_string()),
            virtual_daa_score: Some(get_server_info_response.virtual_daa_score),
            error: if !is_synced { Some("Kaspad is not synced".to_string()) } else { None },
        }
    }
}

impl fmt::Display for Health {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let json_str = serde_json::to_string(&self).expect("Failed to serialize");
        write!(f, "{}", json_str)
    }
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = match self {
            HealthStatus::UP => "UP",
            HealthStatus::WARN => "WARN",
            HealthStatus::DOWN => "DOWN",
        };
        write!(f, "{}", status_str)
    }
}
