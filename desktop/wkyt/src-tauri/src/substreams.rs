//! Local projections over the already-unlocked vault; no external effects.
use std::sync::Arc;
use serde::Deserialize;
use serde_json::{json, Value};
use wkyt_core::{AuthorizationPolicy, CapabilityInvocation, CapabilityManifest, CapabilityResult};
use wkyt_vault::{SavedSubstream, StreamQuery};
use crate::vault_commands::AppState;

pub fn manifests() -> Vec<CapabilityManifest> {
    let query_schema = json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "text": {"type": "string", "maxLength": 1024},
            "connector_ids": {"type": "array", "maxItems": 32, "items": {"type": "string"}},
            "kind": {"description": "Optional serialized ItemKind; null means all kinds"},
            "time_axis": {"enum": ["event", "recorded"]},
            "from": {"type": ["string", "null"], "format": "date-time"},
            "to": {"type": ["string", "null"], "format": "date-time"},
            "as_of": {"type": ["integer", "null"], "minimum": 1},
            "include_deleted": {"type": "boolean"},
            "limit": {"type": "integer", "minimum": 1, "maximum": 200}
        }
    });
    [
        ("core.query_stream", "Search history", "Read a bounded view of retained item versions in the unlocked vault. Returns a committed boundary, coverage start, source identities, and truncation status; no writes or external effects.", query_schema.clone(), json!({"type": "object"})),
        ("core.list_substreams", "List saved views", "Read saved definitions from the unlocked vault; no external effects.", json!({"type": "object", "additionalProperties": false}), json!({"type": "array"})),
        ("core.save_substream", "Save view", "Create or revise a query definition in the encrypted vault, retaining its revision history. Does not copy sources or grant access. The definition write is atomic; a subsequent list-read failure does not undo it. No external effects.", json!({"type": "object", "additionalProperties": false, "required": ["id", "name", "query"], "properties": {"id": {"type": "string", "minLength": 1, "maxLength": 128}, "name": {"type": "string", "minLength": 1, "maxLength": 256}, "query": query_schema}}), json!({"type": "array"})),
        ("core.delete_substream", "Remove saved view", "Soft-delete only the query definition in the encrypted vault. Retains definition history and all source records; no external effects. Saving the same ID can restore it.", json!({"type": "object", "additionalProperties": false, "required": ["id"], "properties": {"id": {"type": "string"}}}), json!({"type": "array"})),
    ].into_iter().map(|(id, name, description, inputs_schema, outputs_schema)| CapabilityManifest {
        id: id.into(), name: name.into(), description: description.into(), inputs_schema, outputs_schema,
        authorization_policy: AuthorizationPolicy::AutoApprove,
    }).collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteArgs { id: String }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}
fn parse<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|_| "Invalid saved-view arguments".into())
}

pub async fn invoke(state: tauri::State<'_, Arc<AppState>>, invocation: CapabilityInvocation) -> Result<CapabilityResult, String> {
    let vault = state.cached_vault().ok_or("Vault is not unlocked")?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut vault = vault.lock().map_err(|_| "Vault is unavailable")?;
        let result = match invocation.capability_id.as_str() {
            "core.query_stream" => {
                let query: StreamQuery = parse(invocation.arguments)?;
                serde_json::to_value(vault.query_stream(&query).map_err(|_| "Invalid query or unavailable history boundary")?)
            }
            "core.list_substreams" => {
                let _: EmptyArgs = parse(invocation.arguments)?;
                serde_json::to_value(vault.saved_substreams().map_err(|_| "Could not read saved views")?)
            }
            "core.save_substream" => {
                let saved: SavedSubstream = parse(invocation.arguments)?;
                vault.save_substream(&saved).map_err(|_| "Could not save this view; check its name, query, and boundary")?;
                serde_json::to_value(vault.saved_substreams().map_err(|_| "Could not read saved views")?)
            }
            "core.delete_substream" => {
                let args: DeleteArgs = parse(invocation.arguments)?;
                vault.delete_substream(&args.id).map_err(|_| "Could not remove this view")?;
                serde_json::to_value(vault.saved_substreams().map_err(|_| "Could not read saved views")?)
            }
            _ => return Err("Unknown saved-view capability".into()),
        };
        Ok(CapabilityResult { data: result.map_err(|_| "Could not encode saved-view result")? })
    }).await.map_err(|_| "Saved-view operation could not complete".to_string())?
}
