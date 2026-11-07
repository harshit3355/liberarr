//! `liberarr status`: known devices (serial suffix only), last pull, queued and failed work.
use super::Outcome;
use crate::container::{App, keys};
use liberarr_core::device::ports::KeyStore;
use liberarr_core::recordings::{RecordingRepo, RecordingStatus};
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
        println!("Appareils : aucun (lancez `liberarr link`)");
    } else {
        println!("Appareils : {}", devices.join(", "));
    }
    match app.db.last_pull().await? {
        Some(t) => println!(
            "Dernier pull : {}",
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M")
        ),
        None => println!("Dernier pull : jamais"),
    }
    let items = app.db.list_items().await?;
    let count = |s| items.iter().filter(|i| i.status == s).count();
    println!(
        "Enregistrements : {} (en cours : {}, en échec : {})",
        items.len(),
        count(RecordingStatus::Processing),
        count(RecordingStatus::Failed)
    );
    Ok(ExitCode::SUCCESS)
}
