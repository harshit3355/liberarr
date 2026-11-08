//! `liberarr status`: known devices (serial suffix only), last pull, queued and failed work.
use super::Outcome;
use crate::container::{App, keys};
use liberarr_core::device::ports::KeyStore;
use liberarr_core::recordings::{RecordingRepo, RecordingStatus};
use rust_i18n::t;
use std::process::ExitCode;

pub async fn run(app: &App) -> Outcome {
    let devices: Vec<String> = keys(&app.cfg)
        .load()?
        .iter()
        .map(|m| {
            format!(
                "…{}",
                m.serial
                    .get(m.serial.len().saturating_sub(4)..)
                    .unwrap_or("")
            )
        })
        .collect();
    if devices.is_empty() {
        println!("{}", t!("status.devices_none"));
    } else {
        println!("{}", t!("status.devices", list = devices.join(", ")));
    }
    match app.db.last_pull().await? {
        Some(at) => println!(
            "{}",
            t!(
                "status.last_pull",
                when = at
                    .to_zoned(jiff::tz::TimeZone::system())
                    .strftime("%Y-%m-%d %H:%M")
            )
        ),
        None => println!("{}", t!("status.never_pulled")),
    }
    let items = app.db.list_items().await?;
    let count = |s| items.iter().filter(|i| i.status == s).count();
    println!(
        "{}",
        t!(
            "status.recordings",
            total = items.len(),
            processing = count(RecordingStatus::Processing),
            failed = count(RecordingStatus::Failed)
        )
    );
    let silent = items.iter().filter(|i| i.no_speech).count();
    if silent > 0 {
        println!("{}", t!("no_speech.status", count = silent));
    }
    Ok(ExitCode::SUCCESS)
}
