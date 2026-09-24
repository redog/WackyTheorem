//! End-to-end pipeline: files on disk → FileImporter → bounded bus →
//! encrypted sqlcipher vault, with ack-after-commit and cursor resume.

use std::fs;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use wkyt_connector_file::FileImporter;
use wkyt_core::{Item, SyncToken};
use wkyt_host::run_pipeline_once;
use wkyt_vault::{KeyService, MemoryKekStore, Vault};

struct Rig {
    _vault_dir: TempDir,
    watch_dir: TempDir,
    vault: Arc<Mutex<Vault>>,
    connector: FileImporter,
}

fn rig() -> Rig {
    let vault_dir = tempfile::tempdir().unwrap();
    let watch_dir = tempfile::tempdir().unwrap();
    let svc = KeyService::new(MemoryKekStore::default(), vault_dir.path());
    let (dek, _recovery) = svc.provision().unwrap();
    let vault = Vault::open(&vault_dir.path().join("vault.db"), &dek).unwrap();
    let connector = FileImporter::new("file-import", watch_dir.path().to_path_buf());
    Rig {
        _vault_dir: vault_dir,
        watch_dir,
        vault: Arc::new(Mutex::new(vault)),
        connector,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn drop_modify_delete_lands_in_encrypted_vault() {
    let r = rig();

    // 1. Drop two files → both land.
    fs::write(r.watch_dir.path().join("notes.json"), r#"{"note": "hello"}"#).unwrap();
    fs::write(r.watch_dir.path().join("cal.ics"), "BEGIN:VCALENDAR\nEND:VCALENDAR").unwrap();

    let stats = run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap();
    assert_eq!(stats.batches_applied, 1);
    assert_eq!(stats.deltas_applied, 6); // 2 files + 2 claims + 2 rels
    {
        let v = r.vault.lock().unwrap();
        assert_eq!(v.item_count().unwrap(), 6);
        let items = v.items("file-import").unwrap();
        let notes = items.iter().find(|i| i.source_id == "notes.json").unwrap();
        assert_eq!(notes.properties["content"]["note"], "hello");
        assert_eq!(
            notes.id,
            Item::deterministic_id("file-import", "notes.json").to_string()
        );
        assert!(v.cursor("file-import").unwrap().is_some(), "cursor committed with the batch");
    }

    let before_edit = r.vault.lock().unwrap().query_stream(&wkyt_vault::StreamQuery::default()).unwrap().boundary.sequence;

    // 2. Idle pass: nothing changed, nothing applied.
    let stats = run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap();
    assert_eq!(stats, wkyt_host::PipelineStats::default());

    // 3. Modify → in-place update, no duplicate row.
    fs::write(r.watch_dir.path().join("notes.json"), r#"{"note": "edited"}"#).unwrap();
    let stats = run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap();
    assert_eq!(stats.deltas_applied, 3); // file + claim + rel updated
    {
        let v = r.vault.lock().unwrap();
        assert_eq!(v.item_count().unwrap(), 6, "modification must not duplicate");
        let items = v.items("file-import").unwrap();
        let notes = items.iter().find(|i| i.source_id == "notes.json").unwrap();
        assert_eq!(notes.properties["content"]["note"], "edited");
        let historical = v.query_stream(&wkyt_vault::StreamQuery {
            as_of: Some(before_edit), ..Default::default()
        }).unwrap();
        let old_notes = historical.items.iter().find(|v| v.item.source_id == "notes.json").unwrap();
        assert_eq!(old_notes.item.properties["content"]["note"], "hello");
    }

    // 4. A source deletion retires its generated claim and link atomically.
    fs::remove_file(r.watch_dir.path().join("cal.ics")).unwrap();
    let deletion = run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap();
    assert_eq!(deletion.deltas_applied, 3);
    let deleted_boundary;
    {
        let v = r.vault.lock().unwrap();
        assert_eq!(v.item_count().unwrap(), 3, "unrelated notes and their derivations remain");
        let claims = v.temporal_claims_with_evidence().unwrap();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].0.source_id, "notes.json-claim");
        assert_eq!(claims[0].1[0].source_id, "notes.json");
        let current = v.query_stream(&wkyt_vault::StreamQuery {
            include_deleted: true, ..Default::default()
        }).unwrap();
        deleted_boundary = current.boundary.sequence;
        let retired: Vec<_> = current.items.iter().filter(|i| i.deleted_at.is_some()).collect();
        assert_eq!(retired.len(), 3);
        assert!(retired.iter().all(|i| i.revision == deleted_boundary));
        let old = v.query_stream(&wkyt_vault::StreamQuery {
            as_of: Some(before_edit), ..Default::default()
        }).unwrap();
        assert_eq!(old.items.len(), 6);
        assert!(old.items.iter().all(|i| i.deleted_at.is_none()));
        assert!(old.items.iter().any(|i| i.item.source_id == "cal.ics" && i.item.raw_payload.is_some()));
    }
    assert_eq!(run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap(), wkyt_host::PipelineStats::default());
    assert_eq!(r.vault.lock().unwrap().query_stream(&Default::default()).unwrap().boundary.sequence, deleted_boundary);

    // 5. Reappearance restores the same three identities, without duplicates.
    fs::write(r.watch_dir.path().join("cal.ics"), "BEGIN:VCALENDAR\nEND:VCALENDAR").unwrap();
    run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap();
    let v = r.vault.lock().unwrap();
    assert_eq!(v.item_count().unwrap(), 6);
    assert_eq!(v.temporal_claims_with_evidence().unwrap().len(), 2);
    assert!(v.items("file-import").unwrap().iter().any(|i|
        i.id == Item::deterministic_id("file-import", "cal.ics").to_string()));

}

#[tokio::test(flavor = "multi_thread")]
async fn corrupted_cursor_triggers_one_full_resync_without_duplicates() {
    let r = rig();
    fs::write(r.watch_dir.path().join("a.json"), "{}").unwrap();
    run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap();

    // Sabotage the stored cursor (simulates a cursor-format change or a
    // source-side token expiry). The pump must fall back to a full resync.
    {
        let mut v = r.vault.lock().unwrap();
        v.apply_batch(&wkyt_core::DeltaBatch {
            connector_id: "file-import".into(),
            deltas: vec![],
            cursor: Some(SyncToken("garbage-not-json".into())),
        })
        .unwrap();
    }

    let stats = run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap();
    assert_eq!(stats.deltas_applied, 3, "full resync re-delivers the file");
    let v = r.vault.lock().unwrap();
    assert_eq!(v.item_count().unwrap(), 3, "resync over existing data must not duplicate");
}

#[tokio::test(flavor = "multi_thread")]
async fn many_files_flow_through_bounded_batches_with_per_batch_commits() {
    let r = rig();
    for i in 0..150 {
        fs::write(r.watch_dir.path().join(format!("f{i:03}.json")), "{}").unwrap();
    }

    let stats = run_pipeline_once(&r.connector, Arc::clone(&r.vault)).await.unwrap();
    assert_eq!(stats.deltas_applied, 450); // 150 * 3
    assert!(
        stats.batches_applied >= 3,
        "150 files at batch size 64 must arrive as multiple bounded batches"
    );
    assert_eq!(r.vault.lock().unwrap().item_count().unwrap(), 450);
}
