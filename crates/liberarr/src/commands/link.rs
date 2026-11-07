//! `liberarr link [--force]`: one-time fetch of the device keys from the Plaud account, then a
//! test unlock. The token is read hidden, used for this run only and never written or logged.
use super::{Outcome, ask};
use crate::config::Config;
use crate::container::keys;
use liberarr_core::device::link::{LinkDevice, Unlock};
use liberarr_infra::ble::{BtleConnector, BtleScanner, default_adapter};
use liberarr_infra::plaud_cloud::PlaudCloud;
use rust_i18n::t;
use std::process::ExitCode;

/// The web app's token is often copied with its `Bearer ` prefix.
fn bare_token(pasted: &str) -> &str {
    let t = pasted.trim();
    t.get(..7)
        .filter(|p| p.eq_ignore_ascii_case("bearer "))
        .map_or(t, |_| t[7..].trim())
}

pub async fn run(cfg: &Config, force: bool) -> Outcome {
    let pasted = ask(&t!("link.token_prompt"), true)?;
    let token = bare_token(&pasted);
    if token.is_empty() {
        return Err(t!("link.no_token").into_owned().into());
    }
    let adapter = default_adapter().await?;
    let (scanner, connector) = (
        BtleScanner::new(adapter.clone()),
        BtleConnector::new(adapter),
    );
    eprintln!("{}", t!("link.fetching"));
    let report = LinkDevice {
        account: &PlaudCloud::new(),
        keys: &keys(cfg),
        scanner: &scanner,
        connector: &connector,
    }
    .run(token, force)
    .await?;
    println!(
        "{}",
        if report.key_pair_fetched {
            t!("link.keys_saved_new_pair", devices = report.devices)
        } else {
            t!("link.keys_saved", devices = report.devices)
        }
    );
    match report.unlock {
        Unlock::Ok => {
            println!("{}", t!("link.unlock_ok"));
            Ok(ExitCode::SUCCESS)
        }
        Unlock::NotSeen => {
            println!("{}", t!("link.not_seen"));
            Ok(ExitCode::from(2))
        }
        Unlock::Failed(why) => {
            println!("{}", t!("link.unlock_failed", why = why));
            Ok(ExitCode::FAILURE)
        }
    }
}
