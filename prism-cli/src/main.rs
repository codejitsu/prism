use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "prism", version = "0.1.0", about = "Agentic PR review tool", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Clone)]
enum Command {
    /// Review a PR
    Review {
        /// The PR number to review
        #[arg(value_name = "pr")]
        pr: i32,
    },
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    match args.command {
        Some(Command::Review { pr }) => {
            log::info!("Reviewing PR: {}", pr);
        }

        None => {
            log::warn!("No command provided");
        }
    }
}
