//! `liberarr process <files…>`: ingests local audio files and runs the pipeline on them.
use super::{Outcome, process_and_report};
use crate::container::App;
use liberarr_core::jobs::JobQueue;
use liberarr_core::recordings::Source;
use std::path::PathBuf;
use std::process::ExitCode;

pub async fn run(app: &App, files: &[PathBuf]) -> Outcome {
    // Once, before any claim: requeues jobs of a dead process and marks this one as a worker.
    app.db.reset_stale().await?;
    let (mut ids, mut ok) = (Vec::new(), true);
    for f in files {
        let title = f.file_stem().map(|s| s.to_string_lossy().into_owned());
        match app.ingest().run(f, Source::File, title, None).await {
            Ok((id, true)) => {
                println!("{} → #{id}", f.display());
                ids.push(id);
            }
            Ok((id, false)) => {
                println!("{} → #{id} (déjà connu)", f.display());
                ids.push(id);
            }
            Err(e) => {
                ok = false;
                println!("{} : {e}", f.display());
            }
        }
    }
    ok &= process_and_report(app, &ids).await?;
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
