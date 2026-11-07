use clap::Parser;

/// Free your Plaud recordings: transcript, summary and visuals, with any AI provider.
#[derive(Parser)]
#[command(version)]
struct Cli {}

fn main() {
    Cli::parse();
}
