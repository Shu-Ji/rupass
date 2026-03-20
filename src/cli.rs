use std::ffi::OsString;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::app;
use crate::storage::validate_team_name;

#[derive(Debug)]
pub(crate) enum ParsedCli {
    Standard(Cli),
    TeamScoped(TeamCommandInput),
}

#[derive(Debug)]
pub(crate) struct TeamCommandInput {
    pub(crate) team: Option<String>,
    pub(crate) command: TeamScopedCommands,
}

#[derive(Parser, Debug)]
#[command(
    name = "rupass",
    version,
    about = "轻量级团队密码管理工具",
    override_usage = "rupass <COMMAND>\n       rupass <team> get <key>",
    long_about = "rupass 是一个轻量级团队密码管理工具。\n日常管理通过 TUI 完成，CLI 只保留读取和全量同步入口。",
    after_help = "命令示例:\n  rupass get db_password\n  rupass this_is_a_test_team get db_password\n  rupass sync-all\n  rupass tui\n\n说明:\n  `get` 读取密钥值。\n  如果本地只有一个团队，可省略 team；如果有多个团队，必须显式传入。\n  团队创建、删除、写入、更新、remote 配置等操作请使用 `rupass tui`。"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    #[command(
        name = "tui",
        about = "启动交互式终端界面",
        after_help = "示例:\n  rupass tui"
    )]
    Tui,
    #[command(
        name = "sync-all",
        about = "同步所有团队仓库",
        after_help = "示例:\n  rupass sync-all"
    )]
    SyncAll,
}

#[derive(Parser, Debug)]
#[command(
    name = "rupass",
    version,
    about = "团队作用域读取命令",
    long_about = "使用 `rupass <team> get <key>` 在指定团队下读取密钥值。",
    after_help = "示例:\n  rupass this_is_a_test_team get db_password"
)]
pub(crate) struct ExplicitTeamScopedCli {
    #[arg(help = "团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
    #[command(subcommand)]
    pub(crate) command: TeamScopedCommands,
}

#[derive(Parser, Debug)]
#[command(
    name = "rupass",
    version,
    about = "默认团队读取命令",
    long_about = "当本地只有一个团队时，可省略 team 直接读取密钥值。",
    after_help = "示例:\n  rupass get db_password"
)]
pub(crate) struct ImplicitTeamScopedCli {
    #[command(subcommand)]
    pub(crate) command: TeamScopedCommands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum TeamScopedCommands {
    #[command(
        about = "读取密钥值",
        after_help = "示例:\n  rupass this_is_a_test_team get db_password"
    )]
    Get(TeamScopedSecretKeyArgs),
}

#[derive(Args, Debug)]
pub(crate) struct TeamScopedSecretKeyArgs {
    #[arg(help = "要读取的 key 名称")]
    pub(crate) key: String,
}

pub(crate) fn run() -> Result<()> {
    let cli = parse_from(std::env::args_os()).unwrap_or_else(|err| err.exit());
    app::dispatch(cli)
}

fn parse_from<I, T>(args: I) -> std::result::Result<ParsedCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let argv: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if should_parse_explicit_team_scoped(&argv) {
        return ExplicitTeamScopedCli::try_parse_from(argv).map(|cli| {
            ParsedCli::TeamScoped(TeamCommandInput {
                team: Some(cli.team),
                command: cli.command,
            })
        });
    }
    if should_parse_implicit_team_scoped(&argv) {
        return ImplicitTeamScopedCli::try_parse_from(argv).map(|cli| {
            ParsedCli::TeamScoped(TeamCommandInput {
                team: None,
                command: cli.command,
            })
        });
    }
    Cli::try_parse_from(argv).map(ParsedCli::Standard)
}

fn should_parse_explicit_team_scoped(argv: &[OsString]) -> bool {
    let Some(first_arg) = argv.get(1).and_then(|arg| arg.to_str()) else {
        return false;
    };
    !first_arg.starts_with('-') && validate_team_name(first_arg).is_ok()
}

fn should_parse_implicit_team_scoped(argv: &[OsString]) -> bool {
    let Some(first_arg) = argv.get(1).and_then(|arg| arg.to_str()) else {
        return false;
    };
    matches!(first_arg, "get")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_team_scoped_get_command() {
        let cli = parse_from(["rupass", "dev_team", "get", "db_password"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team.as_deref(), Some("dev_team"));
                match team_cli.command {
                    TeamScopedCommands::Get(args) => assert_eq!(args.key, "db_password"),
                }
            }
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_implicit_team_scoped_get_command() {
        let cli = parse_from(["rupass", "get", "db_password"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team, None);
                match team_cli.command {
                    TeamScopedCommands::Get(args) => assert_eq!(args.key, "db_password"),
                }
            }
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_sync_all_command() {
        let cli = parse_from(["rupass", "sync-all"]).unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::SyncAll => {}
                command => panic!("unexpected command: {command:?}"),
            },
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_tui_command() {
        let cli = parse_from(["rupass", "tui"]).unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Tui => {}
                command => panic!("unexpected command: {command:?}"),
            },
            other => panic!("unexpected cli: {other:?}"),
        }
    }
}
