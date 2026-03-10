use std::process;

use clap::{Parser, Subcommand};

mod ai;
mod config;
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

        /// Force interpretation as a commit SHA (disambiguates all-digit hashes)
        #[arg(long, short = 'c', conflicts_with = "pr")]
        commit: bool,

        /// Force interpretation as a PR number
        #[arg(long, short = 'p', conflicts_with = "commit")]
        pr: bool,

        /// Enable AI-powered review sections
        #[arg(long)]
        ai: bool,

        /// Override AI model (used only with --ai)
        #[arg(long, requires = "ai")]
        model: Option<String>,
    },

    /// Initialize configuration file at ~/.config/prism/config.toml
    Init,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    match args.command {
        Some(Command::Review {
            target,
            commit,
            pr,
            ai,
            model,
        }) => {
            let cfg = match config::Config::load() {
                Ok(cfg) => cfg,
                Err(e) => {
                    log::error!("Failed to load config: {:#}", e);
                    process::exit(1);
                }
            };

            if let Err(e) = review::review(&target, commit, pr, ai, model.as_deref(), &cfg).await {
                log::error!("{:#}", e);
                process::exit(1);
            }
        }
        Some(Command::Init) => match config::init_config() {
            Ok(path) => {
                println!("Created config file at {}", path.display());
                println!();
                println!("Edit this file to add your GitHub token and OpenAI API key.");
            }
            Err(e) => {
                log::error!("{:#}", e);
                process::exit(1);
            }
        },
        None => {
            log::warn!("No command provided. Run 'prism --help' for usage.");
        }
    }
}
