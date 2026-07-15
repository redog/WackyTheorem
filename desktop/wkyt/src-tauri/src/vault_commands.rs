//! Tauri command surface for the vault: status/ceremony state machine,
//! viewer queries, and the ingestion-pipeline starter.
//!
//! State machine driven by the frontend:
//!
//! ```text
//! vault_status ──first_run──> begin_first_run ──> (key shown once)
//!      │                          └─> verify_recovery_key ──ok──> READY
//!      ├──ready────────────────────────────────────────────────> READY
//!      └──keychain_lost──> recover_with_key ─────────────ok────> READY
//! ```
//!
//! READY = vault cached in state + ingestion pipeline running. The
//! pipeline NEVER starts before the ceremony verifies (or recovery
//! proves the user holds the key): until then the vault stays empty,
//! which is exactly what makes an abandoned ceremony safely resettable.
//!
//! Security notes: the recovery key string crosses to the webview once,
//! at the ceremony (documented D8/D12 trade-off — the user must see it);
//! it is never logged and never stored. Errors crossing to the UI are
//! strings and carry no key material.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use wkyt_connector_file::FileImporter;
use wkyt_connector_google::GoogleCalendarConnector;
use wkyt_core::{CapabilityInvocation, CapabilityManifest, CapabilityResult};
use wkyt_vault::{unlock_vault, KeyError, KeyService, KeyState, DynamicKekStore, Vault};

const KEYRING_SERVICE: &str = "wkyt";
const META_RECOVERY_VERIFIED: &str = "recovery_verified";

pub struct AppState {
    data_dir: PathBuf,
    db_path: PathBuf,
    import_dir: PathBuf,
    /// Outer mutex guards set/replace; inner is the vault's own op lock.
    vault: Mutex<Option<Arc<Mutex<Vault>>>>,
    pipeline_started: AtomicBool,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            db_path: data_dir.join("vault.db"),
            import_dir: data_dir.join("import"),
            data_dir,
            vault: Mutex::new(None),
            pipeline_started: AtomicBool::new(false),
        }
    }

    fn key_service(&self) -> KeyService<DynamicKekStore> {
        KeyService::new(DynamicKekStore::select(KEYRING_SERVICE, &self.data_dir), &self.data_dir)
    }

    pub(crate) fn cached_vault(&self) -> Option<Arc<Mutex<Vault>>> {
        self.vault.lock().unwrap().clone()
    }

    fn cache_vault(&self, vault: Vault) -> Arc<Mutex<Vault>> {
        let arc = Arc::new(Mutex::new(vault));
        *self.vault.lock().unwrap() = Some(Arc::clone(&arc));
        arc
    }
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum VaultStatus {
    /// No vault yet (or a ceremony was abandoned pre-verification):
    /// run `begin_first_run`.
    FirstRun,
    Ready { live_items: i64 },
    /// Wrapped DEK exists but the OS keychain entry is gone:
    /// `recover_with_key`.
    KeychainLost,
    Inconsistent { reason: String },
    NeedsPassphrase { is_new: bool },
}

#[derive(Serialize)]
pub struct VaultStats {
    pub live_items: i64,
    pub import_dir: String,
}

/// Viewer projection of an Item: everything except `raw_payload`, which
/// can be megabytes and has no business in a list view.
#[derive(Serialize)]
pub struct ItemView {
    pub id: String,
    pub connector_id: String,
    pub source_id: String,
    pub kind: serde_json::Value,
    pub timestamp: String,
    pub ingested_at: String,
    pub properties: serde_json::Value,
}

#[derive(Serialize)]
pub struct EvidenceView {
    pub source_id: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct ClaimView {
    pub id: String,
    pub topic: String,
    pub claim: String,
    pub time_range: (String, String),
    pub confidence: String,
    pub epistemic_state: String,
    pub evidence: Vec<EvidenceView>,
}

#[derive(Serialize)]
pub struct RevisionView {
    pub revision_id: i64,
    pub replaced_at: String,
    pub properties: serde_json::Value,
}

fn verified(vault: &Arc<Mutex<Vault>>) -> Result<bool, String> {
    Ok(vault
        .lock()
        .unwrap()
        .get_meta(META_RECOVERY_VERIFIED)
        .map_err(|e| e.to_string())?
        .as_deref()
        == Some("true"))
}

fn start_pipeline(app: &tauri::AppHandle, state: &AppState) {
    if state.pipeline_started.swap(true, Ordering::SeqCst) {
        return;
    }
    let vault = state.cached_vault().expect("pipeline started before vault ready");
    let connector = FileImporter::new("file-import", state.import_dir.clone());
    println!("[wkyt] watching {:?} — drop .json/.ics files there", state.import_dir);
    let _app = app.clone(); // reserved for emitting ingest events to the UI later

    // File importer loop (existing)
    let vault_file = Arc::clone(&vault);
    tauri::async_runtime::spawn(async move {
        loop {
            match wkyt_host::run_pipeline_once(&connector, Arc::clone(&vault_file)).await {
                Ok(stats) if stats.batches_applied > 0 => {
                    println!(
                        "[wkyt] file: ingested {} deltas in {} batches",
                        stats.deltas_applied, stats.batches_applied
                    );
                }
                Ok(_) => {}
                Err(e) => eprintln!("[wkyt] file pipeline pass failed: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });

    // Google Calendar connector loop (only if client_id is configured)
    let client_id = option_env!("WKYT_GOOGLE_CLIENT_ID")
        .map(|s| s.to_string())
        .or_else(|| std::env::var("WKYT_GOOGLE_CLIENT_ID").ok());
    let client_secret = option_env!("WKYT_GOOGLE_CLIENT_SECRET")
        .map(|s| s.to_string())
        .or_else(|| std::env::var("WKYT_GOOGLE_CLIENT_SECRET").ok());

    if let Some(client_id) = client_id {
        let vault_google = Arc::clone(&vault);
        let google = GoogleCalendarConnector::new(client_id, client_secret);
        tauri::async_runtime::spawn(async move {
            // Initial delay: let the file pipeline settle first.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            loop {
                match wkyt_host::run_pipeline_once(&google, Arc::clone(&vault_google)).await {
                    Ok(stats) if stats.batches_applied > 0 => {
                        println!(
                            "[wkyt] google: ingested {} deltas in {} batches",
                            stats.deltas_applied, stats.batches_applied
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        // AuthRequired is expected before the user logs in;
                        // don't spam the console.
                        let msg = e.to_string();
                        if !msg.contains("AuthRequired") && !msg.contains("authentication") {
                            eprintln!("[wkyt] google pipeline pass failed: {e}");
                        }
                    }
                }
                // Poll less frequently than the file importer: calendar
                // data changes slowly and we don't want to burn API quota.
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            }
        });
    }
}

#[tauri::command]
pub async fn vault_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<VaultStatus, String> {
    let s = Arc::clone(&state);
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Already unlocked this session?
        if let Some(vault) = s.cached_vault() {
            return if verified(&vault)? {
                start_pipeline(&app2, &s);
                let live = vault.lock().unwrap().item_count().map_err(|e| e.to_string())?;
                Ok(VaultStatus::Ready { live_items: live })
            } else {
                // Ceremony in flight or abandoned: the UI restarts it.
                Ok(VaultStatus::FirstRun)
            };
        }

        let svc = s.key_service();
        if svc.store().is_passphrase_fallback() && !svc.store().has_passphrase() {
            return Ok(VaultStatus::NeedsPassphrase {
                is_new: !s.db_path.exists(),
            });
        }
        match svc.state(s.db_path.exists()).map_err(|e| e.to_string())? {
            KeyState::FirstRun => Ok(VaultStatus::FirstRun),
            KeyState::KeychainLost => Ok(VaultStatus::KeychainLost),
            KeyState::Inconsistent(reason) => {
                Ok(VaultStatus::Inconsistent { reason: reason.to_string() })
            }
            KeyState::Ready => {
                let (vault, _dek) = unlock_vault(&svc, &s.db_path).map_err(|e| e.to_string())?;
                let vault = s.cache_vault(vault);
                if verified(&vault)? {
                    start_pipeline(&app2, &s);
                    let live = vault.lock().unwrap().item_count().map_err(|e| e.to_string())?;
                    Ok(VaultStatus::Ready { live_items: live })
                } else {
                    // Provisioned but the ceremony never verified — the key
                    // was shown once and is unrecoverable. The vault is
                    // guaranteed empty (pipeline gates on verification), so
                    // begin_first_run will reset and re-provision.
                    Ok(VaultStatus::FirstRun)
                }
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Provision (or safely re-provision after an abandoned ceremony) and
/// return the recovery key for display. The ONLY time it ever crosses to
/// the UI.
#[tauri::command]
pub async fn begin_first_run(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let s = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let svc = s.key_service();

        // Abandoned-ceremony reset path. Hard guards: never reset a vault
        // that has verified its ceremony or that contains any data.
        if let Some(vault) = s.vault.lock().unwrap().take() {
            if verified(&vault)? {
                *s.vault.lock().unwrap() = Some(vault);
                return Err("vault is already provisioned and verified".into());
            }
            let live = vault.lock().unwrap().item_count().map_err(|e| e.to_string())?;
            if live > 0 {
                *s.vault.lock().unwrap() = Some(vault);
                return Err("refusing to reset: vault contains data".into());
            }
            drop(vault); // close the connection before deleting the file
            svc.reset_for_reprovision().map_err(|e| e.to_string())?;
            std::fs::remove_file(&s.db_path).map_err(|e| e.to_string())?;
        } else if s.db_path.exists() || !matches!(svc.state(false), Ok(KeyState::FirstRun)) {
            return Err("vault is already provisioned; refusing to overwrite".into());
        }

        let (dek, recovery) = svc.provision().map_err(|e| e.to_string())?;
        let vault = Vault::open(&s.db_path, &dek).map_err(|e| e.to_string())?;
        vault
            .put_meta(META_RECOVERY_VERIFIED, "false")
            .map_err(|e| e.to_string())?;
        s.cache_vault(vault);
        Ok(recovery.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// D8 ceremony verification: the user re-enters the key; success is the
/// input authenticating the recovery blob. Only then does ingestion start.
#[tauri::command]
pub async fn verify_recovery_key(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    input: String,
) -> Result<(), String> {
    let s = Arc::clone(&state);
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let svc = s.key_service();
        svc.verify_recovery(&input).map_err(friendly_key_error)?;
        let vault = s.cached_vault().ok_or("no vault is being provisioned")?;
        vault
            .lock()
            .unwrap()
            .put_meta(META_RECOVERY_VERIFIED, "true")
            .map_err(|e| e.to_string())?;
        start_pipeline(&app2, &s);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Keychain-loss path: the recovery key re-establishes the keychain wrapper
/// and unlocks. Presenting the key IS proof of possession, so the ceremony
/// flag is set in the same step.
#[tauri::command]
pub async fn recover_with_key(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    input: String,
) -> Result<(), String> {
    let s = Arc::clone(&state);
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let svc = s.key_service();
        let dek = svc.recover(&input).map_err(friendly_key_error)?;
        let vault = Vault::open(&s.db_path, &dek).map_err(|e| e.to_string())?;
        vault
            .put_meta(META_RECOVERY_VERIFIED, "true")
            .map_err(|e| e.to_string())?;
        s.cache_vault(vault);
        start_pipeline(&app2, &s);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_items(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<u32>,
) -> Result<Vec<ItemView>, String> {
    let s = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let vault = s.cached_vault().ok_or("vault is not unlocked")?;
        let items = vault
            .lock()
            .unwrap()
            .recent_items(limit.unwrap_or(200))
            .map_err(|e| e.to_string())?;
        Ok(items
            .into_iter()
            .map(|i| ItemView {
                id: i.id,
                connector_id: i.connector_id,
                source_id: i.source_id,
                kind: serde_json::to_value(&i.kind).unwrap_or_default(),
                timestamp: i.timestamp.to_rfc3339(),
                ingested_at: i.ingested_at.to_rfc3339(),
                properties: i.properties,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn query_claims(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<ClaimView>, String> {
    let s = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let vault = s.cached_vault().ok_or("vault is not unlocked")?;
        let results = vault
            .lock()
            .unwrap()
            .temporal_claims_with_evidence()
            .map_err(|e| e.to_string())?;

        Ok(results
            .into_iter()
            .map(|(claim, evidence)| {
                let topic = claim
                    .properties
                    .get("source")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown")
                    .to_string();

                let assertion = claim
                    .properties
                    .get("assertion")
                    .and_then(|a| a.as_str())
                    .unwrap_or("Unknown claim")
                    .to_string();

                let time_str = claim.timestamp.to_rfc3339();

                ClaimView {
                    id: claim.id,
                    topic,
                    claim: assertion,
                    time_range: (time_str.clone(), time_str),
                    confidence: "High".to_string(),
                    epistemic_state: claim.properties.get("epistemic_type").and_then(|s| s.as_str()).unwrap_or("unknown").to_string(),
                    evidence: evidence
                        .into_iter()
                        .map(|e| {
                            let content = if let Some(c) = e.properties.get("summary") {
                                format!("Calendar event: {}", c.as_str().unwrap_or("Unknown"))
                            } else if let Some(c) = e.properties.get("filename") {
                                format!("File: {}", c.as_str().unwrap_or("Unknown"))
                            } else {
                                e.source_id.clone()
                            };
                            EvidenceView {
                                source_id: e.source_id,
                                content,
                            }
                        })
                        .collect(),
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_capabilities() -> Result<Vec<CapabilityManifest>, String> {
    Ok(vec![
        CapabilityManifest {
            id: "core.query_claims".into(),
            name: "Query Claims".into(),
            description: "Retrieves knowledge claims with their associated evidence.".into(),
            inputs_schema: serde_json::json!({ "type": "object", "properties": {} }),
            outputs_schema: serde_json::json!({ "type": "array" }),
            side_effects: false,
        }
    ])
}

#[tauri::command]
pub async fn invoke_capability(
    state: tauri::State<'_, Arc<AppState>>,
    invocation: CapabilityInvocation,
) -> Result<CapabilityResult, String> {
    match invocation.capability_id.as_str() {
        "core.query_claims" => {
            let claims = query_claims(state).await?;
            Ok(CapabilityResult {
                data: serde_json::to_value(claims).unwrap_or_default(),
            })
        }
        _ => Err(format!("Unknown capability: {}", invocation.capability_id)),
    }
}

#[tauri::command]
pub async fn get_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<VaultStats, String> {
    let s = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let vault = s.cached_vault().ok_or("vault is not unlocked")?;
        let live = vault.lock().unwrap().item_count().map_err(|e| e.to_string())?;
        Ok(VaultStats {
            live_items: live,
            import_dir: s.import_dir.display().to_string(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn query_claim_revisions(
    state: tauri::State<'_, Arc<AppState>>,
    item_id: String,
) -> Result<Vec<RevisionView>, String> {
    let s = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        let vault = s.cached_vault().ok_or("vault is not unlocked")?;
        let revs = vault
            .lock()
            .unwrap()
            .item_revisions(&item_id)
            .map_err(|e| e.to_string())?;

        Ok(revs
            .into_iter()
            .map(|r| RevisionView {
                revision_id: r.revision_id,
                replaced_at: r.replaced_at.to_rfc3339(),
                properties: r.properties,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Key errors phrased for humans, with the deliberate property that wrong
/// key / tampered blob / format drift all read the same (no oracle).
fn friendly_key_error(e: KeyError) -> String {
    match e {
        KeyError::MalformedRecoveryKey => {
            "A recovery key is 64 hex characters (dashes and spaces are fine). \
             Check what you entered."
                .into()
        }
        KeyError::IntegrityFailure => {
            "That key does not match this vault. Check for typos and try again.".into()
        }
        other => other.to_string(),
    }
}

#[tauri::command]
pub async fn set_passphrase(
    state: tauri::State<'_, Arc<AppState>>,
    passphrase: String,
) -> Result<(), String> {
    let s = Arc::clone(&state);
    s.key_service().store().set_passphrase(&passphrase);
    Ok(())
}
