use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::Display;
use std::str::FromStr;

// Re-export SeaORM models as our domain models
pub use crate::entities::users::Model as User;
pub use crate::entities::nodes::Model as Node;
pub use crate::entities::servers::Model as Server;
pub use crate::entities::images::Model as Image;
pub use crate::entities::runtimes::Model as Runtime;
pub use crate::entities::allocations::Model as Allocation;
pub use crate::entities::sessions::Model as Session;


#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Variable {
    pub name: String,
    pub description: String,
    pub env_variable: String,
    pub default_value: String,
    pub user_viewable: bool,
    pub user_editable: bool,
    pub rules: String,
    pub field_type: String, // text, boolean, etc.
}

#[derive(Deserialize)]
pub struct CreateAllocationRequest {
    pub ip: String,
    pub ports: String,
}

#[derive(Deserialize)]
pub struct DeleteAllocationRequest {
    pub ports: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Deserialize)]
pub struct CreateServerRequest {
    // Core
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    pub start_on_install: Option<String>, // Checkbox sends "on" or nothing

    // Allocations
    pub node_id: Option<String>,
    pub default_allocation: Option<String>, // Allocation ID
    pub additional_ports: Option<String>,

    // Feature Limits
    pub backup_limit: Option<i32>,

    // Resource Management
    pub cpu_limit: Option<i32>,
    pub cpu_pinning: Option<String>,
    pub ram_limit: Option<i32>,
    pub swap_limit: Option<i32>,
    pub disk_limit: Option<i32>,
    pub io_weight: Option<i32>,
    pub oom_killer: Option<String>, // Checkbox

    // Image
    pub runtime_id: String,
    pub image_id: String,

    // Docker
    pub docker_image: Option<String>,
    pub custom_docker_image: Option<String>,

    // Startup
    pub startup_command: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateNodeRequest {
    pub name: String,
    pub ip: String,
    pub port: i32,
    #[serde(default = "default_sftp_port")]
    pub sftp_port: i32,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ram_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub disk_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub cpu_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub allocation_ports: Option<String>,
}

fn default_sftp_port() -> i32 {
    2022
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskDetail {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub is_removable: bool,
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub node_id: String,
    pub cpu_usage: f32,
    pub ram_usage: u64,
    pub ram_total: u64,
    #[serde(default)]
    pub disk_usage: u64,
    #[serde(default)]
    pub disk_total: u64,
    pub uptime: u64,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub disk_read: u64,
    #[serde(default)]
    pub disk_write: u64,
    #[serde(default)]
    pub net_rx: u64,
    #[serde(default)]
    pub net_tx: u64,
    #[serde(default)]
    pub disks: Vec<DiskDetail>,
}

#[derive(Deserialize)]
pub struct UpdateNodeRequest {
    pub name: String,
    pub ip: String,
    pub port: i32,
    #[serde(default = "default_sftp_port")]
    pub sftp_port: i32,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ram_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub disk_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub cpu_limit: Option<i32>,
}

fn empty_string_as_none<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(s) if !s.is_empty() => s.parse::<T>().map(Some).map_err(serde::de::Error::custom),
        _ => Ok(None),
    }
}


#[derive(Deserialize)]
pub struct UpdateServerRequest {
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Option<String>,
    
    // Limits
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub cpu_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub ram_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub disk_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub swap_limit: Option<i32>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub backup_limit: Option<i32>,

    // Config
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub io_weight: Option<i32>,
    
    pub oom_killer: Option<String>, // "on" or null
    pub docker_image: Option<String>,
    pub startup_command: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteServerRequest {
    pub force: Option<String>,
}

