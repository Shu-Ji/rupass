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
    disable_help_subcommand = true,
    about = "轻量级团队密码管理工具",
    override_usage = "rupass <COMMAND>\n       rupass <team> get <key>",
    long_about = "rupass 是一个轻量级团队密码管理工具。\n支持 TUI 和完整 CLI，便于脚本或 AI 通过命令行管理团队、密钥与同步。",
    after_help = "示例:\n  rupass tui\n  rupass team list\n  rupass team create dev_team --password secret\n  rupass key set --team dev_team db_password hello123\n  rupass get db_password\n  rupass this_is_a_test_team get db_password"
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
    #[command(
        name = "team",
        about = "团队管理命令",
        after_help = "示例:\n  rupass team list\n  rupass team create dev_team --password secret\n  rupass team set-remote dev_team git@github.com:org/repo.git"
    )]
    Team {
        #[command(subcommand)]
        command: TeamCommands,
    },
    #[command(
        name = "key",
        about = "密钥管理命令",
        after_help = "示例:\n  rupass key list --team dev_team\n  rupass key get --team dev_team db_password\n  rupass key set --team dev_team db_password hello123"
    )]
    Key {
        #[command(subcommand)]
        command: KeyCommands,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum TeamCommands {
    #[command(about = "列出所有团队")]
    List,
    #[command(about = "创建团队")]
    Create(TeamCreateArgs),
    #[command(about = "删除团队")]
    Delete(TeamPasswordTargetArgs),
    #[command(about = "设置团队远程仓库")]
    SetRemote(TeamSetRemoteArgs),
    #[command(about = "清空团队远程仓库")]
    ClearRemote(TeamPasswordTargetArgs),
    #[command(about = "同步指定团队")]
    Sync(TeamPasswordTargetArgs),
}

#[derive(Subcommand, Debug)]
pub(crate) enum KeyCommands {
    #[command(about = "列出团队下所有 key")]
    List(KeyListArgs),
    #[command(about = "读取 key 的值")]
    Get(KeyGetArgs),
    #[command(about = "设置 key 的值")]
    Set(KeySetArgs),
    #[command(about = "删除 key")]
    Delete(KeyDeleteArgs),
}

#[derive(Args, Debug)]
pub(crate) struct TeamCreateArgs {
    #[arg(help = "团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
    #[arg(long, help = "团队密码；不传则交互输入")]
    pub(crate) password: Option<String>,
    #[arg(long = "password-confirm", help = "确认密码；不传则默认与 --password 相同，或交互输入")]
    pub(crate) password_confirm: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct TeamPasswordTargetArgs {
    #[arg(help = "团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct TeamSetRemoteArgs {
    #[arg(help = "团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
    #[arg(help = "远程仓库地址")]
    pub(crate) url: String,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct KeyListArgs {
    #[arg(long, short, help = "团队英文名；不传时会自动推断默认团队")]
    pub(crate) team: Option<String>,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct KeyGetArgs {
    #[arg(help = "要读取的 key 名称")]
    pub(crate) key: String,
    #[arg(long, short, help = "团队英文名；不传时会自动推断默认团队")]
    pub(crate) team: Option<String>,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct KeySetArgs {
    #[arg(help = "要设置的 key 名称")]
    pub(crate) key: String,
    #[arg(help = "要写入的 value")]
    pub(crate) value: String,
    #[arg(long, short, help = "团队英文名；不传时会自动推断默认团队")]
    pub(crate) team: Option<String>,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct KeyDeleteArgs {
    #[arg(help = "要删除的 key 名称")]
    pub(crate) key: String,
    #[arg(long, short, help = "团队英文名；不传时会自动推断默认团队")]
    pub(crate) team: Option<String>,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
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
    fn parses_team_create_command() {
        let cli = parse_from(["rupass", "team", "create", "dev_team", "--password", "secret"]).unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Team { command } => match command {
                    TeamCommands::Create(args) => {
                        assert_eq!(args.team, "dev_team");
                        assert_eq!(args.password.as_deref(), Some("secret"));
                    }
                    other => panic!("unexpected team command: {other:?}"),
                },
                command => panic!("unexpected command: {command:?}"),
            },
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_key_set_command() {
        let cli = parse_from([
            "rupass",
            "key",
            "set",
            "db_password",
            "hello123",
            "--team",
            "dev_team",
        ])
        .unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Key { command } => match command {
                    KeyCommands::Set(args) => {
                        assert_eq!(args.key, "db_password");
                        assert_eq!(args.value, "hello123");
                        assert_eq!(args.team.as_deref(), Some("dev_team"));
                    }
                    other => panic!("unexpected key command: {other:?}"),
                },
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
