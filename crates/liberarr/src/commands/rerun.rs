//! `liberarr rerun <id> [--step]`: queues a step again (the failed one by default), never twice.
use super::Outcome;
use crate::container::App;
use liberarr_core::jobs::{Rerun, rerun};
use liberarr_core::recordings::RecordingRepo;
use rust_i18n::t;
use std::process::ExitCode;

pub async fn run(app: &App, id: i64, step: Option<&str>) -> Outcome {
    if RecordingRepo::get(&app.db, id).await?.is_none() {
        eprintln!("{}", t!("rerun.unknown_id", id = id));
        return Ok(ExitCode::FAILURE);
    }
    match rerun(&app.db, id, step).await? {
        Rerun::Queued(step) => {
            println!("{}", t!("rerun.queued", id = id, step = step));
            Ok(ExitCode::SUCCESS)
        }
        Rerun::Busy => {
            eprintln!("{}", t!("rerun.busy", id = id));
            Ok(ExitCode::FAILURE)
        }
        Rerun::NothingFailed => {
            eprintln!("{}", t!("rerun.nothing_failed", id = id));
            Ok(ExitCode::FAILURE)
        }
    }
}
