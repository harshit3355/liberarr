//! Live device check (spec §6.8), read-only: scan, unlock, list the sessions, then download one
//! starting at `size / 2` and report whether the first data offset equals the requested start.
//! Writes nothing but commands 1, 26 and 28 (and the unlock frames) to the device.
//!
//! Usage: `cargo run -p liberarr-infra --example ble_probe [KEYS_DIR]`
//! KEYS_DIR defaults to `~/.config/liberarr-test/plaud-keys` and holds `sn_signature.json`,
//! `user_rsa_private.pem`, `user_rsa_public.pem` and `bind_token`. Secrets are never printed, and
//! the serial only as its last 4 characters.
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use liberarr_core::device::ports::{DeviceConnector, DeviceScanner, DeviceSession, UnlockMaterial};
use liberarr_infra::ble::{BtleConnector, BtleScanner, default_adapter};
use std::path::{Path, PathBuf};

type R<T> = Result<T, Box<dyn std::error::Error>>;

fn load(dir: &Path) -> R<UnlockMaterial> {
    let sigs: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&std::fs::read(dir.join("sn_signature.json"))?)?;
    let (serial, sig) = sigs.iter().next().ok_or("sn_signature.json is empty")?;
    Ok(UnlockMaterial {
        serial: serial.clone(),
        signature: STANDARD.decode(sig.as_str().ok_or("signature is not a string")?)?,
        private_pem: std::fs::read_to_string(dir.join("user_rsa_private.pem"))?,
        public_pem: std::fs::read_to_string(dir.join("user_rsa_public.pem"))?,
        bind_token: std::fs::read_to_string(dir.join("bind_token"))?
            .trim()
            .to_string(),
    })
}

#[tokio::main]
async fn main() -> R<()> {
    let dir = match std::env::args().nth(1) {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(std::env::var("HOME")?).join(".config/liberarr-test/plaud-keys"),
    };
    let material = load(&dir)?;
    let adapter = default_adapter().await?;

    println!("scanning 15 s (device awake, phone app closed)…");
    let seen = BtleScanner::new(adapter.clone()).scan(15).await?;
    println!("{} Plaud device(s) advertising", seen.len());
    let ad = seen
        .iter()
        .find(|a| a.serial.eq_ignore_ascii_case(&material.serial))
        .ok_or("the device with the stored serial is not advertising")?;
    println!("found {ad:?}");

    let mut session = BtleConnector::new(adapter).open(ad, &material).await?;
    println!("unlocked and handshaken");

    let sessions = session.list().await?;
    println!("{} session(s) on the device:", sessions.len());
    for (id, size) in &sessions {
        println!("  session {id}: {size} B");
    }

    // The smallest session of at least 1 KiB keeps the check short.
    let (id, size) = sessions
        .iter()
        .copied()
        .filter(|(_, s)| *s >= 1024)
        .min_by_key(|(_, s)| *s)
        .ok_or("no session of at least 1 KiB to test the resume with")?;
    let start = size / 2;
    println!("downloading session {id} from offset {start} of {size}");

    let (mut first, mut next, mut bytes, mut contiguous) = (None, start, 0usize, true);
    session
        .download(id, start, size, &mut |offset, data| {
            first.get_or_insert(offset);
            contiguous &= offset == next;
            next = offset.wrapping_add(data.len() as u32);
            bytes += data.len();
            Ok(())
        })
        .await?;

    println!("requested start:      {start}");
    println!("first data offset:    {first:?}");
    println!(
        "first offset == start: {}",
        if first == Some(start) { "yes" } else { "no" }
    );
    println!("bytes received:       {bytes} (expected {})", size - start);
    println!("contiguous:           {contiguous}");
    Ok(())
}
