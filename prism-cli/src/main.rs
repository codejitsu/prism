use std::process;

use clap::{Parser, Subcommand};

mod github;
mod review;

#[derive(Parser)]
#[command(name = "prism", version = "0.1.0", about = "Agentic PR review tool", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Clone)]
enum Command {
    /// Review a PR, commit, or GitHub PR URL
    Review {
        /// PR number, GitHub PR URL, or commit SHA to review
        #[arg(value_name = "target")]
        target: String,
    },
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    match args.command {
        Some(Command::Review { target }) => {
            if let Err(e) = review::review(&target).await {
                log::error!("{:#}", e);
                process::exit(1);
            }
        }
        None => {
            log::warn!("No command provided. Run 'prism --help' for usage.");
        }
    }
}
