mod app;
mod cli;
mod config;
mod db;
mod domain;
mod http;
mod mcp;

use anyhow::Result;
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
    /// Manage bearer tokens for MCP clients
    Token {
        #[command(subcommand)]
        command: cli::token::TokenCommand,
    },
    /// Manage households and their members
    Household {
        #[command(subcommand)]
        command: cli::household::HouseholdCommand,
    },
    /// Users who have logged in
    User {
        #[command(subcommand)]
        command: cli::user::UserCommand,
    },
    /// Re-derive the accounting day of every entry from the location history
    RecomputeDays {
        /// Only this person (email); default everyone
        #[arg(long)]
        user: Option<String>,
    },
}

async fn connect() -> Result<Db> {
    Db::connect(&config::database_url_from_env()?).await
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    match cli.command {
        Command::Serve => http::serve(config::Config::from_env()?).await,
        Command::Migrate { status } => cli::migrate::run(&connect().await?, status).await,
        Command::Token { command } => cli::token::run(&connect().await?, command).await,
        Command::Household { command } => cli::household::run(&connect().await?, command).await,
        Command::User { command } => cli::user::run(&connect().await?, command).await,
        Command::RecomputeDays { user } => cli::recompute::run(&connect().await?, user).await,
    }
}
