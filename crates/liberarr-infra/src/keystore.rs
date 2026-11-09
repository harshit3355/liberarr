//! Encrypted device key store (spec §6.6): XChaCha20-Poly1305 under a key derived from
//! `LIBERARR_SECRET_KEY`. File layout: `nonce[24] ‖ ciphertext`, plaintext = JSON.
//! Errors never carry key material or file content.
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chacha20poly1305::aead::{Aead, Generate, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use liberarr_core::device::ports::{KeyStore, UnlockMaterial};
use liberarr_core::{Error, Result};
use serde_json::{Value, json};
use sha2::Sha256;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

const HKDF_INFO: &[u8] = b"liberarr device keys v1";
const NONCE_LEN: usize = 24;

fn err(msg: &str) -> Error {
    Error::Storage(format!("device keys: {msg}"))
}

pub struct FileKeyStore {
    path: PathBuf,
    aead: XChaCha20Poly1305,
}

impl FileKeyStore {
    /// `path` is `<home>/device/keys.bin`; `secret_key` is the value of `LIBERARR_SECRET_KEY`.
    pub fn new(path: PathBuf, secret_key: &str) -> Self {
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(None, secret_key.as_bytes())
            .expand(HKDF_INFO, &mut key)
            .expect("32 bytes is a valid HKDF-SHA256 output length");
        Self {
            path,
            aead: XChaCha20Poly1305::new(&key.into()),
        }
    }
}

fn to_json(m: &UnlockMaterial) -> Value {
    json!({
        "serial": m.serial,
        "signature": STANDARD.encode(&m.signature),
        "private_pem": m.private_pem,
        "public_pem": m.public_pem,
        "bind_token": m.bind_token,
    })
}

fn from_json(v: &Value) -> Option<UnlockMaterial> {
    let s = |k: &str| Some(v.get(k)?.as_str()?.to_string());
    Some(UnlockMaterial {
        serial: s("serial")?,
        signature: STANDARD.decode(v.get("signature")?.as_str()?).ok()?,
        private_pem: s("private_pem")?,
        public_pem: s("public_pem")?,
        bind_token: s("bind_token")?,
    })
}

impl KeyStore for FileKeyStore {
    fn load(&self) -> Result<Vec<UnlockMaterial>> {
        let raw = match std::fs::read(&self.path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(_) => return Err(err("cannot read the key file")),
        };
        let (nonce, ct) = raw
            .split_first_chunk::<NONCE_LEN>()
            .ok_or_else(|| err("key file is truncated"))?;
        let plain = self.aead.decrypt(&XNonce::from(*nonce), ct).map_err(|_| {
            err("cannot decrypt (wrong LIBERARR_SECRET_KEY, or the file is corrupt)")
        })?;
        let all: Vec<Value> =
            serde_json::from_slice(&plain).map_err(|_| err("key file content is malformed"))?;
        all.iter()
            .map(|v| from_json(v).ok_or_else(|| err("key file entry is malformed")))
            .collect()
    }

    /// Writes a temp file (0600 from creation) then renames it over the old one: a crash never
    /// leaves a half-written key file.
    fn save(&self, all: &[UnlockMaterial]) -> Result<()> {
        let plain = serde_json::to_vec(&all.iter().map(to_json).collect::<Vec<_>>())
            .map_err(|_| err("cannot serialise"))?;
        let nonce = XNonce::try_generate().map_err(|_| err("no system randomness"))?;
        let ct = self
            .aead
            .encrypt(&nonce, plain.as_slice())
            .map_err(|_| err("cannot encrypt"))?;
        let out = [nonce.as_slice(), &ct].concat();

        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|_| err("cannot create the device folder"))?;
        }
        // A replaced key pair may still be needed for recordings not yet pulled: keep the
        // current file, as is (same encryption), in `keys.bin.prev`.
        // shortcut: one generation only, a second replacement overwrites it; keep a history if
        // replacements become frequent.
        let old = self.load().unwrap_or_default();
        if old
            .iter()
            .any(|o| all.iter().all(|n| n.private_pem != o.private_pem))
        {
            let raw = std::fs::read(&self.path).map_err(|_| err("cannot read the key file"))?;
            write_0600(&self.path.with_extension("bin.prev"), &raw)
                .map_err(|_| err("cannot keep the previous key file"))?;
        }
        write_0600(&self.path, &out).map_err(|_| err("cannot write the key file"))
    }
}

/// Writes a temp file (0600 from creation) then renames it over `path`.
fn write_0600(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let write = || -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    };
    write().inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn material(serial: &str) -> UnlockMaterial {
        UnlockMaterial {
            serial: serial.into(),
            signature: (0..=255).collect(),
            private_pem: "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n".into(),
            public_pem: "-----BEGIN PUBLIC KEY-----\ndef\n-----END PUBLIC KEY-----\n".into(),
            bind_token: "user-id".into(),
        }
    }

    fn dir() -> tempdir::Dir {
        tempdir::Dir::new()
    }

    /// Minimal unique temp dir, removed on drop (no extra dev-dependency).
    mod tempdir {
        use std::path::{Path, PathBuf};
        pub struct Dir(PathBuf);
        impl Dir {
            pub fn new() -> Self {
                let p = std::env::temp_dir().join(format!(
                    "liberarr-keystore-{}-{:?}",
                    std::process::id(),
                    std::thread::current().id()
                ));
                let _ = std::fs::remove_dir_all(&p);
                std::fs::create_dir_all(&p).expect("temp dir");
                Self(p)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn keystore_roundtrip_and_wrong_key_fails() {
        let d = dir();
        let path = d.path().join("device/keys.bin");
        let store = FileKeyStore::new(path.clone(), "secret-one");
        assert!(store.load().unwrap().is_empty(), "absent file = no keys");

        store.save(&[material("aa"), material("bb")]).unwrap();
        let got = store.load().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].serial, "bb");
        assert_eq!(got[0].signature, material("aa").signature);
        assert_eq!(got[0].private_pem, material("aa").private_pem);
        assert_eq!(got[0].bind_token, "user-id");

        // Encrypted at rest: no plaintext field in the file.
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(8).any(|w| w == b"PRIVATE "));
        assert!(!raw.windows(7).any(|w| w == b"user-id"));

        // Wrong key and tampered file: an error, never silent empty keys.
        let other = FileKeyStore::new(path.clone(), "secret-two");
        assert!(other.load().is_err());
        let mut bad = raw;
        *bad.last_mut().unwrap() ^= 1;
        std::fs::write(&path, bad).unwrap();
        assert!(store.load().is_err());
    }

    #[test]
    fn keystore_file_is_0600() {
        let d = dir();
        let path = d.path().join("keys.bin");
        let store = FileKeyStore::new(path.clone(), "k");
        store.save(&[material("aa")]).unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        // A second save (replace) keeps the mode.
        store.save(&[material("bb")]).unwrap();
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn replacing_the_key_pair_keeps_the_previous_file() {
        let d = dir();
        let path = d.path().join("keys.bin");
        let prev = d.path().join("keys.bin.prev");
        let store = FileKeyStore::new(path.clone(), "k");
        store.save(&[material("aa")]).unwrap();
        // Same pair (signature refresh): no backup.
        store.save(&[material("aa")]).unwrap();
        assert!(!prev.exists());

        let old = std::fs::read(&path).unwrap();
        let mut new = material("aa");
        new.private_pem = "new-private".into();
        store.save(&[new]).unwrap();
        assert_eq!(
            std::fs::read(&prev).unwrap(),
            old,
            "same bytes, same encryption"
        );
        let mode = std::fs::metadata(&prev).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let kept = FileKeyStore::new(prev, "k").load().unwrap();
        assert_eq!(kept[0].private_pem, material("aa").private_pem);
        assert_eq!(store.load().unwrap()[0].private_pem, "new-private");
    }
}
