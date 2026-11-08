//! Ports of the device context, shared by `pull` and `link`. Adapters live in `liberarr-infra`.
use super::protocol::Advert;
use crate::Result;
use std::collections::HashSet;
use std::path::PathBuf;

pub trait DeviceScanner {
    /// Listens for `secs` seconds and returns every Plaud device seen.
    async fn scan(&self, secs: u64) -> Result<Vec<Advert>>;
}

/// What it takes to unlock one device. No `Debug`: it holds a private key.
#[derive(Clone)]
pub struct UnlockMaterial {
    pub serial: String,
    pub signature: Vec<u8>,
    pub private_pem: String,
    pub public_pem: String,
    /// The account user id sent in the handshake.
    pub bind_token: String,
}

pub trait DeviceConnector {
    type Session: DeviceSession;
    /// Connects, unlocks and handshakes.
    async fn open(&self, ad: &Advert, unlock: &UnlockMaterial) -> Result<Self::Session>;
}

pub trait DeviceSession {
    /// `(session id, size in bytes)` of every recording on the device.
    async fn list(&mut self) -> Result<Vec<(u32, u32)>>;
    /// Streams `[start, end)` of the encrypted file to `sink(offset, bytes)`, data packets only.
    /// An error from `sink` aborts the download and is returned.
    async fn download(
        &mut self,
        session: u32,
        start: u32,
        end: u32,
        sink: &mut dyn FnMut(u32, &[u8]) -> Result<()>,
    ) -> Result<()>;
}

/// Encrypted at rest by the adapter.
pub trait KeyStore {
    fn load(&self) -> Result<Vec<UnlockMaterial>>;
    fn save(&self, all: &[UnlockMaterial]) -> Result<()>;
}

/// Sessions already pulled, keyed on `(serial_hash, session)`.
pub trait DeviceLedger {
    async fn known(&self, serial_hash: &str) -> Result<HashSet<u32>>;
    async fn record(
        &self,
        serial_hash: &str,
        session: u32,
        size: u32,
        recording_id: i64,
    ) -> Result<()>;
}

/// File-backed scratch space of a pull; `key` is `<serial_hash>-<session>`.
pub trait PartialStore {
    /// Bytes of the `.part` file so far (0 when absent).
    fn len(&self, key: &str) -> Result<u64>;
    fn append(&self, key: &str, data: &[u8]) -> Result<()>;
    /// Reads the whole `.part` (empty when absent); it stays on disk until `discard`, so a crash loses nothing.
    fn take(&self, key: &str) -> Result<Vec<u8>>;
    fn discard(&self, key: &str) -> Result<()>;
    /// Keeps the encrypted file that failed verification, for the owner to inspect.
    fn keep_raw(&self, key: &str, data: &[u8]) -> Result<PathBuf>;
    /// Writes the verified Ogg where `Ingest` can read it (`<home>/audio/device-<key>.ogg`).
    fn stage_audio(&self, key: &str, ogg: &[u8]) -> Result<PathBuf>;
}
