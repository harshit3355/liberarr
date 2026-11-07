//! One module per CLI command. Output is for the owner, in French; it never shows a token, a key,
//! a full serial, a transcript or a summary.
pub mod init;
pub mod link;
pub mod process;
pub mod pull;
pub mod serve;
pub mod status;

use crate::container::App;
use liberarr_core::jobs::{JobQueue, JobStatus};
use std::io::{BufRead, IsTerminal, Write};

pub type Outcome = Result<std::process::ExitCode, Box<dyn std::error::Error>>;

/// Reads one line. `hidden` input is not echoed on a terminal; piped input is read as is.
pub fn ask(prompt: &str, hidden: bool) -> std::io::Result<String> {
    let line = if hidden && std::io::stdin().is_terminal() {
        rpassword::prompt_password(prompt)?
    } else {
        eprint!("{prompt}");
        std::io::stderr().flush()?;
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "entrée fermée",
            ));
        }
        line
    };
    Ok(line.trim().to_string())
}

/// Runs the queue to the end, then prints where each of `ids` stands. `false` if one failed.
pub async fn process_and_report(app: &App, ids: &[i64]) -> Result<bool, liberarr_core::Error> {
    if ids.is_empty() {
        return Ok(true);
    }
    eprintln!("Traitement (transcription, résumé, export)…");
    app.drain().await?;
    let mut ok = true;
    for &id in ids {
        match app.db.list_for(id).await?.last() {
            Some(j) if j.status == JobStatus::Failed => {
                ok = false;
                println!(
                    "  #{id} : échec à l'étape {} — {}",
                    j.step,
                    j.error.as_deref().unwrap_or("?")
                );
            }
            Some(j) if j.status == JobStatus::Done => println!("  #{id} : terminé"),
            _ => println!("  #{id} : en attente"),
        }
    }
    if ok {
        println!("Notes Obsidian : {}", app.cfg.obsidian.display());
    } else {
        println!("Relancez l'étape en échec depuis l'interface (`liberarr serve`).");
    }
    Ok(ok)
}
