use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationPolicy {
    AutoApprove,
    RequireHuman,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityManifest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub inputs_schema: Value,
    pub outputs_schema: Value,
    pub authorization_policy: AuthorizationPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityInvocation {
    pub capability_id: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResult {
    pub data: Value,
}

