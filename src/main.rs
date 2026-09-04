// Until the API and MCP land (steps 3 and 4) most of the database layer has
// no caller outside the tests.
#![allow(dead_code)]

mod app;
mod cli;
mod config;
mod db;
mod domain;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use crate::db::Db;

#[derive(Parser)]
#[command(
    name = "koryto",
    version,
    about = "Calorie and weight log for a household: API, web UI, CLI and MCP server"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server (API, web UI, MCP)
    Serve,
    /// Apply pending migrations, or show their status
    Migrate {
        /// Only list migrations, do not apply anything
        #[arg(long)]
        status: bool,
    },
}

async fn connect() -> Result<Db> {
    Db::connect(&config::database_url_from_env()?).await
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    match cli.command {
        Command::Serve => bail!("not implemented"),
        Command::Migrate { status } => cli::migrate::run(&connect().await?, status).await,
    }
}
