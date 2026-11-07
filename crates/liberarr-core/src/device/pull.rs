//! Pull use case: bring every finished recording off the device, resumably and verified (spec §6.3).
use super::file::decrypt_plaud_file;
use super::ports::*;
use super::protocol::Advert;
use crate::audio::ogg::verify_ogg;
use crate::jobs::JobQueue;
use crate::recordings::{AudioStore, Ingest, RecordingRepo, Source};
use crate::{Error, Result};
use rsa::RsaPrivateKey;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use sha2::{Digest, Sha256};
use std::time::Duration;

const SCAN_SECS: u64 = 12;
/// Gap between the two listings that tells a finished session from one still being recorded.
const SETTLE: Duration = Duration::from_secs(3);

/// Lowercase hex SHA-256 of the serial: the only form of it that reaches the ledger and file names.
pub fn serial_hash(serial: &str) -> String {
    Sha256::digest(serial.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Default, PartialEq)]
pub struct PullReport {
    pub pulled: Vec<i64>,
    /// Sessions whose size changed between the two listings: still being recorded.
    pub skipped_recording: Vec<u32>,
    pub failed: Vec<(u32, String)>,
    pub no_device: bool,
}

pub struct PullRecordings<'a, Sc, Co, K, L, P, R, Q, S> {
    pub scanner: &'a Sc,
    pub connector: &'a Co,
    pub keys: &'a K,
    pub ledger: &'a L,
    pub partials: &'a P,
    pub ingest: Ingest<'a, R, Q, S>,
    /// Zone of the recording title.
    pub tz: jiff::tz::TimeZone,
}

impl<Sc, Co, K, L, P, R, Q, S> PullRecordings<'_, Sc, Co, K, L, P, R, Q, S>
where
    Sc: DeviceScanner,
    Co: DeviceConnector,
    K: KeyStore,
    L: DeviceLedger,
    P: PartialStore,
    R: RecordingRepo,
    Q: JobQueue,
    S: AudioStore,
{
    pub async fn run(&self, sleep: impl Fn(Duration)) -> Result<PullReport> {
        let mut report = PullReport::default();
        let materials = self.keys.load()?;
        let seen = if materials.is_empty() {
            Vec::new()
        } else {
            self.scanner.scan(SCAN_SECS).await?
        };
        // Devices whose serial is unknown are ignored.
        let ours: Vec<_> = seen
            .iter()
            .filter_map(|ad| {
                let m = materials
                    .iter()
                    .find(|m| m.serial.eq_ignore_ascii_case(&ad.serial))?;
                Some((ad, m))
            })
            .collect();
        if ours.is_empty() {
            report.no_device = true;
            return Ok(report);
        }
        for (ad, m) in ours {
            self.pull_device(ad, m, &sleep, &mut report).await?;
        }
        Ok(report)
    }

    async fn pull_device(
        &self,
        ad: &Advert,
        m: &UnlockMaterial,
        sleep: &impl Fn(Duration),
        report: &mut PullReport,
    ) -> Result<()> {
        let key = RsaPrivateKey::from_pkcs8_pem(&m.private_pem)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(&m.private_pem))
            .map_err(|_| Error::Device("stored private key is unreadable".into()))?;
        let hash = serial_hash(&m.serial);
        let mut dev = self.connector.open(ad, m).await?;
        let before = dev.list().await?;
        sleep(SETTLE);
        let after = dev.list().await?;
        let known = self.ledger.known(&hash).await?;

        for (session, size) in after {
            if known.contains(&session) {
                continue;
            }
            // A session that is new or grew since the first listing is still being recorded.
            if !before.contains(&(session, size)) {
                report.skipped_recording.push(session);
                continue;
            }
            let part = format!("{hash}-{session}");
            let ogg = match self.fetch(&mut dev, &key, &part, session, size).await? {
                Ok(ogg) => ogg,
                Err(why) => {
                    report.failed.push((session, why));
                    continue;
                }
            };
            let path = self.partials.stage_audio(&part, &ogg)?;
            let at = jiff::Timestamp::from_second(i64::from(session))
                .map_err(|_| Error::Device("session start out of range".into()))?;
            let title = at
                .to_zoned(self.tz.clone())
                .strftime("%Y-%m-%d %H:%M")
                .to_string();
            let (id, _) = self
                .ingest
                .run(&path, Source::Device, Some(title), Some(at))
                .await?;
            self.ledger.record(&hash, session, size, id).await?;
            // Only now is the `.part` redundant: a crash before this point loses nothing.
            self.partials.discard(&part)?;
            report.pulled.push(id);
        }
        Ok(())
    }

    /// Downloads (resuming from the `.part`), decrypts and verifies one session. The inner `Err`
    /// is a per-session failure to report; the outer one is a storage error that stops the pull.
    async fn fetch(
        &self,
        dev: &mut Co::Session,
        key: &RsaPrivateKey,
        part: &str,
        session: u32,
        size: u32,
    ) -> Result<std::result::Result<Vec<u8>, String>> {
        let mut start = self.partials.len(part)?;
        if start > u64::from(size) {
            self.partials.discard(part)?;
            start = 0;
        }
        let mut start = start as u32; // <= size
        while start < size {
            let (mut next, mut misaligned) = (start, false);
            let mut sink = |offset: u32, data: &[u8]| -> Result<()> {
                if offset != next {
                    misaligned = true;
                    return Err(Error::Device("unexpected data offset".into()));
                }
                self.partials.append(part, data)?;
                next = next.saturating_add(data.len() as u32);
                Ok(())
            };
            match dev.download(session, start, size, &mut sink).await {
                Ok(()) => break,
                // The device ignored the start offset: correctness never depends on resume.
                Err(_) if misaligned && start > 0 => {
                    self.partials.discard(part)?;
                    start = 0;
                }
                // The `.part` stays, so the next pull resumes.
                Err(e) => return Ok(Err(e.to_string())),
            }
        }
        let data = self.partials.take(part)?;
        let checked = if data.len() as u64 != u64::from(size) {
            Err("downloaded size differs from the listed size".to_string())
        } else {
            decrypt_plaud_file(&data, key)
                .map_err(|e| e.to_string())
                .and_then(|ogg| verify_ogg(&ogg).map(|()| ogg).map_err(|e| e.to_string()))
        };
        if checked.is_err() {
            // Never store unverified audio; keep the encrypted file for inspection.
            self.partials.keep_raw(part, &data)?;
            self.partials.discard(part)?;
        }
        Ok(checked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::fakes::*;
    use crate::fakes::{FakeStore, MemJobs, MemRecordings, block_on};
    use jiff::tz::{TimeZone, offset};

    struct Rig {
        dev: FakeDevice,
        keys: MemKeys,
        ledger: MemLedger,
        parts: MemPartials,
        recs: MemRecordings,
        jobs: MemJobs,
    }

    impl Rig {
        fn new(dev: FakeDevice) -> Self {
            let keys = MemKeys::default();
            keys.0.borrow_mut().push(material());
            Self {
                dev,
                keys,
                ledger: MemLedger::default(),
                parts: MemPartials::default(),
                recs: MemRecordings::default(),
                jobs: MemJobs::default(),
            }
        }

        fn pull(&self) -> PullReport {
            let p = PullRecordings {
                scanner: &self.dev,
                connector: &self.dev,
                keys: &self.keys,
                ledger: &self.ledger,
                partials: &self.parts,
                ingest: Ingest {
                    recordings: &self.recs,
                    jobs: &self.jobs,
                    store: &FakeStore,
                },
                tz: TimeZone::fixed(offset(2)),
            };
            block_on(p.run(|_| {})).expect("pull")
        }

        fn part_key() -> String {
            format!("{}-{SESSION}", serial_hash(SERIAL))
        }
    }

    #[test]
    fn pull_reports_no_device() {
        let rig = Rig::new(FakeDevice::default());
        rig.dev
            .0
            .adverts
            .borrow_mut()
            .push(advert("ffffffffffffffff")); // serial not in the key store
        let r = rig.pull();
        assert!(r.no_device);
        assert_eq!(rig.dev.0.opened.get(), 0);
    }

    #[test]
    fn pull_skips_known_sessions() {
        let (_, file) = ogg_and_device_file();
        let rig = Rig::new(FakeDevice::with_file(file));
        rig.ledger.known.borrow_mut().insert(SESSION);
        let r = rig.pull();
        assert_eq!(r, PullReport::default());
        assert!(rig.dev.0.downloads.borrow().is_empty());
    }

    #[test]
    fn pull_skips_growing_session() {
        let (_, file) = ogg_and_device_file();
        let dev = FakeDevice::with_file(file);
        dev.0.grow.set(Some(SESSION));
        let rig = Rig::new(dev);
        let r = rig.pull();
        assert_eq!(r.skipped_recording, vec![SESSION]);
        assert!(r.pulled.is_empty() && r.failed.is_empty());
        assert!(rig.dev.0.downloads.borrow().is_empty());
        assert!(rig.ledger.records.borrow().is_empty());
    }

    #[test]
    fn pull_resumes_after_drop() {
        let (ogg, file) = ogg_and_device_file();
        let dev = FakeDevice::with_file(file);
        dev.0.drop_after.set(Some(300));
        let rig = Rig::new(dev);

        let first = rig.pull();
        assert_eq!(first.failed.len(), 1);
        assert!(first.pulled.is_empty());
        assert_eq!(rig.parts.len(&Rig::part_key()).unwrap(), 300);

        let second = rig.pull();
        assert_eq!(second.pulled, vec![1]);
        assert!(second.failed.is_empty());
        // The second download started where the first stopped.
        assert_eq!(
            *rig.dev.0.downloads.borrow(),
            vec![(SESSION, 0), (SESSION, 300)]
        );
        assert_eq!(rig.parts.staged.borrow()[&Rig::part_key()], ogg);
        assert!(rig.parts.parts.borrow().is_empty());
        assert_eq!(rig.recs.0.borrow().len(), 1);
        // 1_700_000_000 = 2023-11-14T22:13:20Z, shown in the configured zone (+02:00).
        assert_eq!(
            rig.recs.0.borrow()[0].title.as_deref(),
            Some("2023-11-15 00:13")
        );
        assert_eq!(
            rig.recs.0.borrow()[0].created_at.as_second(),
            i64::from(SESSION)
        );
        assert_eq!(rig.ledger.records.borrow().len(), 1);
    }

    #[test]
    fn pull_restarts_when_device_ignores_offset() {
        let (ogg, file) = ogg_and_device_file();
        let dev = FakeDevice::with_file(file.clone());
        dev.0.ignore_offset.set(true);
        let rig = Rig::new(dev);
        rig.parts.append(&Rig::part_key(), &file[..300]).unwrap();

        let r = rig.pull();
        assert_eq!(r.pulled, vec![1]);
        assert_eq!(
            *rig.dev.0.downloads.borrow(),
            vec![(SESSION, 300), (SESSION, 0)]
        );
        assert_eq!(rig.parts.staged.borrow()[&Rig::part_key()], ogg);
    }

    #[test]
    fn pull_keeps_raw_on_bad_ogg() {
        let (_, file) = ogg_and_device_file();
        let mut bad = file.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1; // flips a page byte after decryption: CRC mismatch
        let rig = Rig::new(FakeDevice::with_file(bad.clone()));

        let r = rig.pull();
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].0, SESSION);
        assert!(r.pulled.is_empty());
        assert_eq!(rig.parts.raw.borrow()[&Rig::part_key()], bad);
        assert!(rig.parts.staged.borrow().is_empty());
        assert!(rig.recs.0.borrow().is_empty());
        assert!(rig.ledger.records.borrow().is_empty());
    }
}
