//! `liberarr link [--force]`: one-time fetch of the device keys from the Plaud account, then a
//! test unlock. The token is read hidden, used for this run only and never written or logged.
use super::{Outcome, ask};
use crate::config::Config;
use crate::container::keys;
use liberarr_core::device::link::{LinkDevice, Unlock};
use liberarr_infra::ble::{BtleConnector, BtleScanner, default_adapter};
use liberarr_infra::plaud_cloud::PlaudCloud;
use std::process::ExitCode;

/// The web app's token is often copied with its `Bearer ` prefix.
fn bare_token(pasted: &str) -> &str {
    let t = pasted.trim();
    t.get(..7)
        .filter(|p| p.eq_ignore_ascii_case("bearer "))
        .map_or(t, |_| t[7..].trim())
}

pub async fn run(cfg: &Config, force: bool) -> Outcome {
    let pasted = ask("Token Plaud (saisie masquée) : ", true)?;
    let token = bare_token(&pasted);
    if token.is_empty() {
        return Err("aucun token saisi".into());
    }
    let adapter = default_adapter().await?;
    let (scanner, connector) = (
        BtleScanner::new(adapter.clone()),
        BtleConnector::new(adapter),
    );
    eprintln!("Récupération des clés, puis recherche de l'appareil (environ 12 s)…");
    let report = LinkDevice {
        account: &PlaudCloud::new(),
        keys: &keys(cfg),
        scanner: &scanner,
        connector: &connector,
    }
    .run(token, force)
    .await?;
    println!(
        "Clés enregistrées pour {} appareil(s){}.",
        report.devices,
        if report.key_pair_fetched {
            " (nouvelle paire de clés)"
        } else {
            ""
        }
    );
    match report.unlock {
        Unlock::Ok => {
            println!("Test de déverrouillage réussi. Lancez `liberarr pull`.");
            Ok(ExitCode::SUCCESS)
        }
        Unlock::NotSeen => {
            println!(
                "Aucun appareil détecté pour le test : réveillez-le, fermez l'app Plaud sur le téléphone, puis relancez `liberarr link`."
            );
            Ok(ExitCode::from(2))
        }
        Unlock::Failed(why) => {
            println!(
                "Échec du test de déverrouillage : {why}. Fermez l'app Plaud sur le téléphone et réessayez."
            );
            Ok(ExitCode::FAILURE)
        }
    }
}
