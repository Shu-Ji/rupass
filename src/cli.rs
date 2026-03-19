use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::app;
use crate::storage::{DEFAULT_TEAM_DISPLAY_NAME, DEFAULT_TEAM_NAME};

#[derive(Parser, Debug)]
#[command(
    name = "rupass",
    version,
    about = "A lightweight team-based secret store"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    Init(InitArgs),
    Set(SecretSetArgs),
    Get(SecretKeyArgs),
    Delete(SecretKeyArgs),
    List(SecretListArgs),
    Team {
        #[command(subcommand)]
        command: TeamCommands,
    },
}

#[derive(Args, Debug)]
pub(crate) struct InitArgs {
    #[arg(long, default_value = DEFAULT_TEAM_NAME)]
    pub(crate) team: String,
    #[arg(long, default_value = DEFAULT_TEAM_DISPLAY_NAME)]
    pub(crate) display_name: String,
}

#[derive(Args, Debug)]
pub(crate) struct SecretSetArgs {
    pub(crate) key: String,
    pub(crate) value: String,
    #[arg(long, default_value = DEFAULT_TEAM_NAME)]
    pub(crate) team: String,
}

#[derive(Args, Debug)]
pub(crate) struct SecretKeyArgs {
    pub(crate) key: String,
    #[arg(long, default_value = DEFAULT_TEAM_NAME)]
    pub(crate) team: String,
}

#[derive(Args, Debug)]
pub(crate) struct SecretListArgs {
    #[arg(long, default_value = DEFAULT_TEAM_NAME)]
    pub(crate) team: String,
}

#[derive(Subcommand, Debug)]
pub(crate) enum TeamCommands {
    Create(TeamCreateArgs),
    List,
    SetRemote(TeamRemoteArgs),
    Sync(TeamSyncArgs),
}

#[derive(Args, Debug)]
pub(crate) struct TeamCreateArgs {
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) display_name: Option<String>,
    #[arg(long)]
    pub(crate) remote: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct TeamRemoteArgs {
    #[arg(long)]
    pub(crate) team: String,
    #[arg(long)]
    pub(crate) url: String,
}

#[derive(Args, Debug)]
pub(crate) struct TeamSyncArgs {
    #[arg(long, default_value = DEFAULT_TEAM_NAME)]
    pub(crate) team: String,
}

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    app::dispatch(cli)
}
