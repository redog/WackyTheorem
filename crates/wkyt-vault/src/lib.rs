//! Encrypted SQLite vault (Milestone 2).
//!
//! Implements D12's two-tier key hierarchy and D1/D10's sqlcipher storage:
//!
//! - [`keys::KeyService`] — provisions and unlocks the **DEK** (data
//!   encryption key, what sqlcipher receives) wrapped under two
//!   interchangeable **KEKs**: one held in the OS keychain (silent unlock
//!   on the login session), one derived from the user-held recovery key
//!   (D8 ceremony). Either wrapper opens the vault; losing the keychain is
//!   recoverable without re-encrypting anything.
//! - [`vault::Vault`] — opens the sqlcipher database with a raw-key
//!   `PRAGMA key`, fails closed on wrong key or plaintext files, and
//!   applies the D9 hardening (0600 permissions, in-memory temp store,
//!   sqlcipher memory security).
//!
//! Memory-handling rules (D12): key material lives only in
//! `Zeroizing` buffers, is never formatted into errors or `Debug` output,
//! and the hex/SQL strings momentarily needed for `PRAGMA key` are
//! zeroized immediately after use. What we cannot control is documented
//! at the relevant call sites rather than hidden.

mod hexfmt;
pub mod keys;
pub mod vault;

pub use keys::{Dek, KeyError, KeyService, KeyState, KeyringStore, KekStore, MemoryKekStore, RecoveryKey};
pub use vault::{Vault, VaultError};
