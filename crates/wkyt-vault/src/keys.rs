//! KEK/DEK key service (D12) and recovery wrappers (D8, as amended).
//!
//! Key hierarchy:
//!
//! ```text
//!   OS keychain ──holds──> KEK ──unwraps──> ┐
//!                                            ├─> DEK ──PRAGMA key──> sqlcipher
//!   user (paper) ─holds──> recovery key ───> ┘
//! ```
//!
//! The DEK is wrapped twice with XChaCha20-Poly1305 into two blob files on
//! disk: `dek.keychain.json` (wrapped by the keychain KEK; the silent
//! cold-start path) and `dek.recovery.json` (wrapped by the user-held
//! recovery key; the keychain-loss path). The AEAD's associated data binds
//! each blob to its purpose and format version, so a recovery blob cannot
//! be swapped in for a keychain blob (or vice versa) without detection.
//!
//! Memory rules: every buffer that ever holds key material is
//! `Zeroizing`; `Dek`/`RecoveryKey` redact their `Debug` output; no error
//! variant carries key bytes. Honest limits (D12): the keychain IPC and
//! the OS keychain daemon hold copies we cannot zeroize, and the DEK
//! necessarily lives in sqlcipher's memory while the vault is open —
//! `Vault::open` enables `cipher_memory_security` to make sqlcipher lock
//! and wipe its own crypto buffers.

use crate::hexfmt;
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
    XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zeroize::Zeroizing;

pub const KEY_LEN: usize = 32;
const BLOB_VERSION: u32 = 1;
const XNONCE_LEN: usize = 24;

const KEYCHAIN_BLOB: &str = "dek.keychain.json";
const RECOVERY_BLOB: &str = "dek.recovery.json";
const KEYRING_USER: &str = "vault-kek";

/// The data encryption key: what sqlcipher receives. Exists unwrapped only
/// in process memory; zeroized on drop. Bytes are deliberately reachable
/// only inside this crate (`Vault::open` needs them; nothing else does).
pub struct Dek(Zeroizing<[u8; KEY_LEN]>);

impl Dek {
    fn generate() -> Self {
        Dek(Zeroizing::new(XChaCha20Poly1305::generate_key(&mut OsRng).into()))
    }

    pub(crate) fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for Dek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Dek(<redacted>)")
    }
}

/// The user-held recovery key (the second KEK). Displayed exactly once at
/// the D8 ceremony; zeroized on drop; Debug redacted.
pub struct RecoveryKey(Zeroizing<[u8; KEY_LEN]>);

impl RecoveryKey {
    fn generate() -> Self {
        RecoveryKey(Zeroizing::new(XChaCha20Poly1305::generate_key(&mut OsRng).into()))
    }

    /// Parse user input back into a key, tolerating display formatting.
    pub fn parse(input: &str) -> Result<Self, KeyError> {
        hexfmt::decode_key32_lenient(input)
            .map(|k| RecoveryKey(Zeroizing::new(k)))
            .ok_or(KeyError::MalformedRecoveryKey)
    }

    /// The dash-grouped string shown during the ceremony. Returned inside
    /// `Zeroizing` so the caller's copy is erased on drop too; the UI layer
    /// owns whatever copies the clipboard/renderer make (documented UX
    /// trade-off — the key must reach the user somehow).
    pub fn display(&self) -> Zeroizing<String> {
        let hex = Zeroizing::new(hexfmt::encode(&*self.0));
        Zeroizing::new(hexfmt::group_for_display(&hex))
    }
}

impl std::fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryKey(<redacted>)")
    }
}

/// No key bytes in any variant, ever: errors get logged and shown in UI.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("keychain unavailable or failed: {0}")]
    Keychain(String),
    #[error("no KEK in the OS keychain (keychain lost or never provisioned)")]
    KekMissing,
    #[error("key blob {0:?} is missing")]
    BlobMissing(PathBuf),
    #[error("key blob failed authentication (wrong key, tampered, or corrupt)")]
    IntegrityFailure,
    #[error("recovery key is not in the expected format (64 hex digits)")]
    MalformedRecoveryKey,
    #[error("unsupported key blob version {0}")]
    UnsupportedBlobVersion(u32),
    #[error("key state is inconsistent: {0}")]
    Inconsistent(&'static str),
    #[error("io error on key blob: {0}")]
    Io(#[from] std::io::Error),
    #[error("key blob is not valid JSON: {0}")]
    Format(#[from] serde_json::Error),
}

/// What the caller should do next, decided before touching the DEK.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyState {
    /// Nothing provisioned: run `provision()` and the recovery ceremony.
    FirstRun,
    /// Wrapped DEK + KEK present: `unlock()`.
    Ready,
    /// Wrapped DEK present but the keychain entry is gone (OS reinstall,
    /// keyring wipe): prompt for the recovery key, then `recover()`.
    KeychainLost,
    /// Data exists but key material does not (or only partially). Nothing
    /// can be decrypted without the recovery blob; surface to the operator
    /// rather than guessing.
    Inconsistent(&'static str),
}

/// Where the keychain KEK lives. Production uses [`KeyringStore`]; tests
/// use [`MemoryKekStore`]; D2's headless-Linux passphrase fallback will be
/// a third implementation (M6).
pub trait KekStore {
    fn get(&self) -> Result<Option<Zeroizing<[u8; KEY_LEN]>>, KeyError>;
    fn set(&self, kek: &[u8; KEY_LEN]) -> Result<(), KeyError>;
    fn delete(&self) -> Result<(), KeyError>;
}

/// OS keychain via the `keyring` crate (D2). The KEK is stored hex-encoded
/// (keychains store strings). Note: the string crosses the Secret-Service
/// D-Bus boundary / platform IPC — copies beyond that point are the OS's.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }

    fn entry(&self) -> Result<keyring::Entry, KeyError> {
        keyring::Entry::new(&self.service, KEYRING_USER)
            .map_err(|e| KeyError::Keychain(e.to_string()))
    }
}

impl KekStore for KeyringStore {
    fn get(&self) -> Result<Option<Zeroizing<[u8; KEY_LEN]>>, KeyError> {
        match self.entry()?.get_password() {
            Ok(hex) => {
                let hex = Zeroizing::new(hex);
                let kek = hexfmt::decode_key32_lenient(&hex)
                    .ok_or(KeyError::Inconsistent("keychain entry is not a 256-bit key"))?;
                Ok(Some(Zeroizing::new(kek)))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(KeyError::Keychain(e.to_string())),
        }
    }

    fn set(&self, kek: &[u8; KEY_LEN]) -> Result<(), KeyError> {
        let hex = Zeroizing::new(hexfmt::encode(kek));
        self.entry()?
            .set_password(&hex)
            .map_err(|e| KeyError::Keychain(e.to_string()))
    }

    fn delete(&self) -> Result<(), KeyError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeyError::Keychain(e.to_string())),
        }
    }
}

/// In-memory KEK store for tests and development. Never persists.
#[derive(Default)]
pub struct MemoryKekStore(Mutex<Option<Zeroizing<[u8; KEY_LEN]>>>);

impl KekStore for MemoryKekStore {
    fn get(&self) -> Result<Option<Zeroizing<[u8; KEY_LEN]>>, KeyError> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn set(&self, kek: &[u8; KEY_LEN]) -> Result<(), KeyError> {
        *self.0.lock().unwrap() = Some(Zeroizing::new(*kek));
        Ok(())
    }
    fn delete(&self) -> Result<(), KeyError> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

/// On-disk envelope for a wrapped DEK. Binary fields are hex; the AEAD tag
/// is inside `ct`. `version` and `purpose` are ALSO bound into the AEAD's
/// associated data — editing them here breaks authentication, so the JSON
/// is self-describing but not attacker-malleable.
#[derive(Serialize, Deserialize)]
struct BlobFile {
    version: u32,
    purpose: String, // "keychain" | "recovery"
    nonce: String,   // 24-byte XChaCha nonce, hex
    ct: String,      // ciphertext || Poly1305 tag, hex
}

fn aad_for(purpose: &str) -> Vec<u8> {
    format!("wkyt-dek-blob:v{BLOB_VERSION}:{purpose}").into_bytes()
}

fn wrap(dek: &Dek, kek: &[u8; KEY_LEN], purpose: &str) -> BlobFile {
    let cipher = XChaCha20Poly1305::new(kek.into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, Payload { msg: dek.bytes(), aad: &aad_for(purpose) })
        .expect("XChaCha20-Poly1305 encryption is infallible for 32-byte input");
    BlobFile {
        version: BLOB_VERSION,
        purpose: purpose.to_string(),
        nonce: hexfmt::encode(&nonce),
        ct: hexfmt::encode(&ct),
    }
}

fn unwrap(blob: &BlobFile, kek: &[u8; KEY_LEN], purpose: &str) -> Result<Dek, KeyError> {
    if blob.version != BLOB_VERSION {
        return Err(KeyError::UnsupportedBlobVersion(blob.version));
    }
    let nonce_bytes = hexfmt::decode(&blob.nonce).ok_or(KeyError::IntegrityFailure)?;
    let ct = hexfmt::decode(&blob.ct).ok_or(KeyError::IntegrityFailure)?;
    if nonce_bytes.len() != XNONCE_LEN {
        return Err(KeyError::IntegrityFailure);
    }
    let cipher = XChaCha20Poly1305::new(kek.into());
    // Deliberately opaque on failure: wrong key, bit-flip, and
    // purpose-swap (AAD mismatch) are indistinguishable to a caller.
    let pt = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&nonce_bytes),
                Payload { msg: &ct, aad: &aad_for(purpose) },
            )
            .map_err(|_| KeyError::IntegrityFailure)?,
    );
    if pt.len() != KEY_LEN {
        return Err(KeyError::IntegrityFailure);
    }
    let mut dek = Zeroizing::new([0u8; KEY_LEN]);
    dek.copy_from_slice(&pt);
    Ok(Dek(dek))
}

pub struct KeyService<S: KekStore> {
    store: S,
    keychain_blob: PathBuf,
    recovery_blob: PathBuf,
}

impl<S: KekStore> KeyService<S> {
    pub fn new(store: S, data_dir: &Path) -> Self {
        Self {
            store,
            keychain_blob: data_dir.join(KEYCHAIN_BLOB),
            recovery_blob: data_dir.join(RECOVERY_BLOB),
        }
    }

    /// Decide the cold-start path. `db_exists` is the caller's check on the
    /// vault file (the key service deliberately doesn't know the DB path).
    pub fn state(&self, db_exists: bool) -> Result<KeyState, KeyError> {
        let kc = self.keychain_blob.exists();
        let rc = self.recovery_blob.exists();
        let kek = self.store.get()?.is_some();

        Ok(match (db_exists, kc, rc, kek) {
            // Fresh install. A keychain blob is written last during
            // provisioning, so its absence with no DB means any other
            // debris (orphan KEK, lone recovery blob from an interrupted
            // provision) is safely overwritten by re-provisioning.
            (false, false, _, _) => KeyState::FirstRun,
            (_, true, _, true) => KeyState::Ready,
            (_, true, true, false) => KeyState::KeychainLost,
            (_, true, false, false) => {
                KeyState::Inconsistent("KEK lost and no recovery blob exists")
            }
            (true, false, true, _) => KeyState::KeychainLost,
            (true, false, false, _) => {
                KeyState::Inconsistent("vault exists but all key material is missing")
            }
        })
    }

    /// First-run provisioning. Generates DEK + both KEKs, persists the
    /// wrappers, and returns the recovery key for the D8 ceremony — the
    /// only time it ever exists outside the user's head/paper. The caller
    /// MUST run the ceremony (display + `verify_recovery`) before writing
    /// user data.
    ///
    /// Write order is crash-safe by construction: KEK → recovery blob →
    /// keychain blob (last). A crash anywhere before the final write
    /// leaves `state()` at `FirstRun`, and re-provisioning overwrites the
    /// debris. Only the final rename makes the provisioning visible.
    pub fn provision(&self) -> Result<(Dek, RecoveryKey), KeyError> {
        if self.keychain_blob.exists() {
            return Err(KeyError::Inconsistent(
                "already provisioned; refusing to overwrite key material",
            ));
        }
        let dek = Dek::generate();
        let recovery = RecoveryKey::generate();
        let kek = Zeroizing::new(<[u8; KEY_LEN]>::from(XChaCha20Poly1305::generate_key(
            &mut OsRng,
        )));

        self.store.set(&kek)?;
        write_blob_atomic(&self.recovery_blob, &wrap(&dek, &recovery.0, "recovery"))?;
        write_blob_atomic(&self.keychain_blob, &wrap(&dek, &kek, "keychain"))?;
        Ok((dek, recovery))
    }

    /// The silent cold-start path: keychain KEK unwraps the DEK.
    pub fn unlock(&self) -> Result<Dek, KeyError> {
        let blob = read_blob(&self.keychain_blob)?;
        let kek = self.store.get()?.ok_or(KeyError::KekMissing)?;
        unwrap(&blob, &kek, "keychain")
    }

    /// D8 ceremony verification: proves the user actually saved the key by
    /// requiring them to re-enter it. Success == the input authenticates
    /// the recovery blob; nothing is mutated.
    pub fn verify_recovery(&self, input: &str) -> Result<(), KeyError> {
        let key = RecoveryKey::parse(input)?;
        let blob = read_blob(&self.recovery_blob)?;
        unwrap(&blob, &key.0, "recovery").map(drop)
    }

    /// Keychain-loss recovery: the recovery key unwraps the DEK, then a
    /// fresh KEK is generated, stored in the (new) keychain, and the
    /// keychain blob is re-wrapped. The recovery blob — and the user's
    /// recovery key — remain valid and unchanged.
    pub fn recover(&self, input: &str) -> Result<Dek, KeyError> {
        let key = RecoveryKey::parse(input)?;
        let blob = read_blob(&self.recovery_blob)?;
        let dek = unwrap(&blob, &key.0, "recovery")?;

        let kek = Zeroizing::new(<[u8; KEY_LEN]>::from(XChaCha20Poly1305::generate_key(
            &mut OsRng,
        )));
        self.store.set(&kek)?;
        write_blob_atomic(&self.keychain_blob, &wrap(&dek, &kek, "keychain"))?;
        Ok(dek)
    }
}

fn read_blob(path: &Path) -> Result<BlobFile, KeyError> {
    if !path.exists() {
        return Err(KeyError::BlobMissing(path.to_path_buf()));
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

/// Write-to-temp + rename so a crash can never leave a half-written blob,
/// with 0600 from the moment of creation (D9) on Unix.
fn write_blob_atomic(path: &Path, blob: &BlobFile) -> Result<(), KeyError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        use std::io::Write;
        f.write_all(serde_json::to_string_pretty(blob)?.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(dir: &Path) -> KeyService<MemoryKekStore> {
        KeyService::new(MemoryKekStore::default(), dir)
    }

    #[test]
    fn wrap_unwrap_round_trip_and_purpose_binding() {
        let dek = Dek::generate();
        let kek = [7u8; KEY_LEN];
        let blob = wrap(&dek, &kek, "keychain");

        let back = unwrap(&blob, &kek, "keychain").unwrap();
        assert_eq!(back.bytes(), dek.bytes());

        // Same key, wrong purpose: AAD mismatch must fail closed.
        assert!(matches!(
            unwrap(&blob, &kek, "recovery"),
            Err(KeyError::IntegrityFailure)
        ));
        // Wrong key fails identically (no oracle distinguishing the two).
        assert!(matches!(
            unwrap(&blob, &[8u8; KEY_LEN], "keychain"),
            Err(KeyError::IntegrityFailure)
        ));
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let dek = Dek::generate();
        let kek = [7u8; KEY_LEN];
        let mut blob = wrap(&dek, &kek, "keychain");
        // Flip one nibble of the ciphertext.
        let mut ct = blob.ct.into_bytes();
        ct[0] = if ct[0] == b'0' { b'1' } else { b'0' };
        blob.ct = String::from_utf8(ct).unwrap();
        assert!(matches!(
            unwrap(&blob, &kek, "keychain"),
            Err(KeyError::IntegrityFailure)
        ));
    }

    #[test]
    fn provision_then_unlock_yields_same_dek() {
        let dir = tempfile::tempdir().unwrap();
        let svc = svc(dir.path());
        assert_eq!(svc.state(false).unwrap(), KeyState::FirstRun);

        let (dek, recovery) = svc.provision().unwrap();
        assert_eq!(svc.state(true).unwrap(), KeyState::Ready);
        assert_eq!(svc.unlock().unwrap().bytes(), dek.bytes());

        // Ceremony verification: displayed form round-trips.
        svc.verify_recovery(&recovery.display()).unwrap();
        // And a wrong key does not.
        assert!(svc.verify_recovery(&"0".repeat(64)).is_err());
    }

    #[test]
    fn provision_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let svc = svc(dir.path());
        svc.provision().unwrap();
        assert!(matches!(svc.provision(), Err(KeyError::Inconsistent(_))));
    }

    #[test]
    fn keychain_loss_is_detected_and_recoverable() {
        let dir = tempfile::tempdir().unwrap();
        let svc = svc(dir.path());
        let (dek, recovery) = svc.provision().unwrap();

        svc.store.delete().unwrap(); // simulate keyring wipe / OS reinstall
        assert_eq!(svc.state(true).unwrap(), KeyState::KeychainLost);
        assert!(matches!(svc.unlock(), Err(KeyError::KekMissing)));

        let recovered = svc.recover(&recovery.display()).unwrap();
        assert_eq!(recovered.bytes(), dek.bytes());

        // Recovery re-provisions the keychain path: silent unlock works again.
        assert_eq!(svc.state(true).unwrap(), KeyState::Ready);
        assert_eq!(svc.unlock().unwrap().bytes(), dek.bytes());
    }

    #[test]
    fn blob_swap_attack_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let svc = svc(dir.path());
        let (_, _recovery) = svc.provision().unwrap();
        // Adversary (or confused sync tool) copies the recovery blob over
        // the keychain blob. Even with the right KEK, AAD binding rejects it.
        fs::copy(&svc.recovery_blob, &svc.keychain_blob).unwrap();
        assert!(matches!(svc.unlock(), Err(KeyError::IntegrityFailure)));
    }

    #[test]
    fn debug_output_is_redacted() {
        let dek = Dek::generate();
        let rk = RecoveryKey::generate();
        assert_eq!(format!("{dek:?}"), "Dek(<redacted>)");
        assert_eq!(format!("{rk:?}"), "RecoveryKey(<redacted>)");
    }

    #[cfg(unix)]
    #[test]
    fn blob_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let svc = svc(dir.path());
        svc.provision().unwrap();
        for p in [&svc.keychain_blob, &svc.recovery_blob] {
            let mode = fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{p:?} must be owner-only");
        }
    }
}
