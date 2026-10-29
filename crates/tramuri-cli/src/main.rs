use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tramuri_core::{init, scan};

#[derive(Debug, Parser)]
#[command(name = "tramuri", version, about = "Local-first project knowledge infrastructure")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize Tramuri's local project database.
    Init {
        /// Project directory (defaults to the current directory).
        path: Option<PathBuf>,
        /// Print the result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Inventory project files and persist the latest complete snapshot.
    Scan {
        /// Project directory (defaults to the current directory).
        path: Option<PathBuf>,
        /// Print the result as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("tramuri: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Init { path, json } => {
            let report = init(path.unwrap_or_else(|| PathBuf::from(".")))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if report.already_initialized {
                println!("Tramuri is already initialized in {}", report.project_root);
            } else {
                println!("Initialized Tramuri in {}", report.project_root);
            }
        }
        Command::Scan { path, json } => {
            let report = scan(path.unwrap_or_else(|| PathBuf::from(".")))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Scanned {} files in {} ({})",
                    report.files.len(),
                    report.project_root,
                    if report.complete { "complete" } else { "partial" }
                );
                println!(
                    "Added: {}, changed: {}, unchanged: {}, unverified: {}, deleted: {}, sensitive excluded: {}",
                    report.added.len(),
                    report.changed.len(),
                    report.unchanged.len(),
                    report.unverified.len(),
                    report.deleted.len(),
                    report.sensitive_entries_excluded
                );
                for warning in &report.warnings {
                    let path = warning.path.as_deref().unwrap_or("<project>");
                    eprintln!("warning [{}] {path}: {}", warning.code, warning.message);
                }
            }
        }
    }
    Ok(())
}
