//! Liberarr CLI: parses the command, loads the config, wires the adapters and runs.
mod commands;
mod config;
mod container;
mod i18n;
mod models;
mod providers;
mod setup;
mod tui;

// Catalogs are embedded at compile time: no locale file is read at runtime.
rust_i18n::i18n!("locales", fallback = "en");

use clap::{Parser, Subcommand};
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

/// Free your Plaud recordings: transcript, summary and visuals, with any AI provider.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create the home folder and its .env (generated secrets, AI provider keys).
    Init {
        /// Ask nothing: read LIBERARR_{TRANSCRIBE,LLM}_{PROVIDER,BASE_URL,API_KEY,MODEL} from the
        /// environment (keys never go on the command line) and fail if one is missing.
        #[arg(long)]
        non_interactive: bool,
        /// Question style: `plain` (default) or `tui` (full-screen). Default from LIBERARR_UI.
        #[arg(long, value_parser = ["plain", "tui"])]
        ui: Option<String>,
    },
    /// Fetch the device keys from your Plaud account once, then test an unlock.
    Link {
        /// Fetch a new key pair even if one is stored.
        #[arg(long)]
        force: bool,
    },
    /// Pull new recordings from the device, then process them.
    Pull {
        /// Only pull; do not transcribe or summarize.
        #[arg(long)]
        no_process: bool,
    },
    /// Run the pipeline on local audio files.
    Process {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Start the web UI and API, with the job worker.
    Serve {
        /// Address to bind; 0.0.0.0 exposes the UI on the network.
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        #[arg(long, default_value_t = 8484)]
        port: u16,
    },
    /// Show the known devices, the last pull and queued or failed work.
    Status,
    /// Full-screen terminal app: recordings, transcripts, summaries, pipeline, device pull,
    /// settings. Starts with the setup when there is no configuration yet.
    Tui,
}

fn run(cmd: Cmd) -> commands::Outcome {
    if let Cmd::Init {
        non_interactive,
        ui,
    } = cmd
    {
        let ui = ui.or_else(|| std::env::var("LIBERARR_UI").ok());
        return commands::init::run(non_interactive, ui.as_deref() == Some("tui"));
    }
    if let Cmd::Tui = cmd {
        return tui::run();
    }
    let cfg = config::load(&config::home()?)?;
    let rt = tokio::runtime::Runtime::new()?;
    if let Cmd::Link { force } = cmd {
        return rt.block_on(commands::link::run(&cfg, force));
    }
    for w in container::warnings(&cfg) {
        eprintln!("{w}");
    }
    // Synchronous open (it may wait on a busy database), before any async work.
    let app = container::App::open(cfg)?;
    rt.block_on(async {
        match cmd {
            Cmd::Pull { no_process } => commands::pull::run(&app, no_process).await,
            Cmd::Process { files } => commands::process::run(&app, &files).await,
            Cmd::Serve { host, port } => commands::serve::run(app, host, port).await,
            Cmd::Status => commands::status::run(&app).await,
            Cmd::Init { .. } | Cmd::Link { .. } | Cmd::Tui => unreachable!("handled above"),
        }
    })
}

fn main() -> ExitCode {
    let cmd = Cli::parse().cmd;
    // Once, before any message is printed.
    let lang = config::home().map_or_else(|_| i18n::language(None), |h| i18n::ui_language(&h));
    rust_i18n::set_locale(lang.as_str());
    match run(cmd) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{}", rust_i18n::t!("error.prefix", message = e));
            ExitCode::FAILURE
        }
    }
}
