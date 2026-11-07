//! `liberarr pull [--no-process]`: brings new recordings off the device, then processes them.
use super::{Outcome, process_and_report};
use crate::container::{App, keys, partials};
use liberarr_core::Result;
use liberarr_core::device::ports::{DeviceConnector, DeviceSession, KeyStore, UnlockMaterial};
use liberarr_core::device::protocol::Advert;
use liberarr_core::device::pull::PullRecordings;
use liberarr_core::jobs::JobQueue;
use liberarr_infra::ble::{BtleConnector, BtleScanner, default_adapter};
use rust_i18n::t;
use std::io::Write;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Wraps the connector so every download draws a progress line (bytes, speed, ETA).
struct Progress<C>(C);
struct ProgressSession<S>(S);

impl<C: DeviceConnector> DeviceConnector for Progress<C> {
    type Session = ProgressSession<C::Session>;
    async fn open(&self, ad: &Advert, unlock: &UnlockMaterial) -> Result<Self::Session> {
        Ok(ProgressSession(self.0.open(ad, unlock).await?))
    }
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000.0
}

impl<S: DeviceSession> DeviceSession for ProgressSession<S> {
    async fn list(&mut self) -> Result<Vec<(u32, u32)>> {
        self.0.list().await
    }

    async fn download(
        &mut self,
        session: u32,
        start: u32,
        end: u32,
        sink: &mut dyn FnMut(u32, &[u8]) -> Result<()>,
    ) -> Result<()> {
        // The session id is the recording's start time.
        let label = jiff::Timestamp::from_second(i64::from(session)).map_or_else(
            |_| session.to_string(),
            |t| {
                t.to_zoned(jiff::tz::TimeZone::system())
                    .strftime("%Y-%m-%d %H:%M")
                    .to_string()
            },
        );
        let (t0, total) = (Instant::now(), u64::from(end));
        let (mut got, mut drawn) = (0u64, None::<Instant>);
        let r = self
            .0
            .download(session, start, end, &mut |offset, data| {
                sink(offset, data)?;
                got += data.len() as u64;
                if drawn.is_none_or(|d| d.elapsed() >= Duration::from_millis(250)) {
                    drawn = Some(Instant::now());
                    let speed = got as f64 / t0.elapsed().as_secs_f64().max(0.001);
                    let left = total.saturating_sub(u64::from(start) + got) as f64;
                    let eta = (left / speed.max(1.0)) as u64;
                    eprint!(
                        "\r{}",
                        t!(
                            "pull.progress",
                            label = label,
                            done = format!("{:.1}", mb(u64::from(start) + got)),
                            total = format!("{:.1}", mb(total)),
                            speed = format!("{:.0}", speed / 1000.0),
                            eta = format!("{}:{:02}", eta / 60, eta % 60)
                        )
                    );
                    let _ = std::io::stderr().flush();
                }
                Ok(())
            })
            .await;
        eprintln!();
        r
    }
}

pub async fn run(app: &App, no_process: bool) -> Outcome {
    if !no_process {
        // Once, before any claim: requeues jobs of a dead process and marks this one as a worker.
        app.db.reset_stale().await?;
    }
    let keys = keys(&app.cfg);
    if keys.load()?.is_empty() {
        eprintln!("{}", t!("pull.no_device_linked"));
        return Ok(ExitCode::from(2));
    }
    let adapter = default_adapter().await?;
    let connector = Progress(BtleConnector::new(adapter.clone()));
    eprintln!("{}", t!("pull.searching"));
    let report = PullRecordings {
        scanner: &BtleScanner::new(adapter),
        connector: &connector,
        keys: &keys,
        ledger: &app.db,
        partials: &partials(&app.cfg),
        ingest: app.ingest(),
        tz: jiff::tz::TimeZone::system(),
    }
    .run(|d| tokio::time::sleep(d))
    .await?;

    if report.no_device {
        eprintln!("{}", t!("pull.no_device_in_range"));
        return Ok(ExitCode::from(2));
    }
    println!(
        "{}",
        t!(
            "pull.summary",
            pulled = report.pulled.len(),
            skipped = report.skipped_recording.len(),
            failed = report.failed.len()
        )
    );
    for (session, why) in &report.failed {
        println!(
            "{}",
            t!("pull.failed_session", session = session, why = why)
        );
    }
    for (tail, why) in &report.device_errors {
        println!("{}", t!("pull.device_error", tail = tail, why = why));
    }
    let mut ok = report.failed.is_empty() && report.device_errors.is_empty();
    if !no_process {
        ok &= process_and_report(app, &report.pulled).await?;
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
