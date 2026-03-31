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
    about = "轻量级本地密码管理工具",
    override_usage = "rupass <COMMAND>\n       rupass [team] <COMMAND>",
    long_about = "rupass 是一个轻量级本地密码管理工具。\n支持 TUI 和完整 CLI，便于脚本或 AI 通过命令行管理团队与密钥。",
    after_help = concat!(
        "通用示例:\n",
        "  rupass tui\n",
        "  rupass team list\n",
        "  rupass team create my_team --password secret\n",
        "  rupass team import-file ./finn_team.json --password secret\n",
        "\n",
        "默认团队示例（本地仅有一个团队时）:\n",
        "  rupass list\n",
        "  rupass get db_password\n",
        "  rupass set db_password hello123\n",
        "  rupass del db_password\n",
        "\n",
        "传递团队示例:\n",
        "  rupass my_team list\n",
        "  rupass my_team get db_password\n",
        "  rupass my_team set db_password hello123\n",
        "  rupass my_team del db_password\n"
    )
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
        name = "team",
        about = "团队管理命令",
        after_help = "示例:\n  rupass team list\n  rupass team create my_team --password secret\n  rupass team import-file ./finn_team.json --password secret\n  rupass team del my_team --password secret"
    )]
    Team {
        #[command(subcommand)]
        command: TeamCommands,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum TeamCommands {
    #[command(about = "列出所有团队")]
    List,
    #[command(about = "创建团队")]
    Create(TeamCreateArgs),
    #[command(name = "import-file", about = "从 team.json 导入团队")]
    ImportFile(TeamImportFileArgs),
    #[command(name = "del", about = "删除团队")]
    Del(TeamPasswordTargetArgs),
}

#[derive(Args, Debug)]
pub(crate) struct TeamCreateArgs {
    #[arg(help = "团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
    #[arg(long, help = "团队密码；不传则交互输入")]
    pub(crate) password: Option<String>,
    #[arg(
        long = "password-confirm",
        help = "确认密码；不传则默认与 --password 相同，或交互输入"
    )]
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
pub(crate) struct TeamImportFileArgs {
    #[arg(help = "team.json 文件路径")]
    pub(crate) path: String,
    #[arg(long, help = "团队密码；不传则交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct TeamScopedPasswordArgs {
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct TeamScopedGetArgs {
    #[arg(help = "要读取的 key 名称")]
    pub(crate) key: String,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct TeamScopedSetArgs {
    #[arg(help = "要设置的 key 名称")]
    pub(crate) key: String,
    #[arg(help = "要写入的 value")]
    pub(crate) value: String,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct TeamScopedDeleteArgs {
    #[arg(help = "要删除的 key 名称")]
    pub(crate) key: String,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Parser, Debug)]
#[command(
    name = "rupass",
    version,
    about = "团队作用域读取命令",
    long_about = "使用 `rupass <team> <command>` 在指定团队下管理密钥。",
    after_help = concat!(
        "传递团队示例:\n",
        "  rupass my_team list\n",
        "  rupass my_team get db_password\n",
        "  rupass my_team set db_password hello123\n",
        "  rupass my_team del db_password\n"
    )
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
    about = "默认团队命令",
    long_about = "当本地只有一个团队时，可省略 team 直接管理该团队下的密钥。",
    after_help = concat!(
        "默认团队示例（本地仅有一个团队时）:\n",
        "  rupass list\n",
        "  rupass get db_password\n",
        "  rupass set db_password hello123\n",
        "  rupass del db_password\n"
    )
)]
pub(crate) struct ImplicitTeamScopedCli {
    #[command(subcommand)]
    pub(crate) command: TeamScopedCommands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum TeamScopedCommands {
    #[command(
        about = "列出团队下所有 key",
        after_help = concat!(
            "默认团队示例（本地仅有一个团队时）:\n",
            "  rupass list\n",
            "\n",
            "传递团队示例:\n",
            "  rupass my_team list\n"
        )
    )]
    List(TeamScopedPasswordArgs),
    #[command(
        about = "读取密钥值",
        after_help = concat!(
            "默认团队示例（本地仅有一个团队时）:\n",
            "  rupass get db_password\n",
            "\n",
            "传递团队示例:\n",
            "  rupass my_team get db_password\n"
        )
    )]
    Get(TeamScopedGetArgs),
    #[command(
        about = "设置密钥值",
        after_help = concat!(
            "默认团队示例（本地仅有一个团队时）:\n",
            "  rupass set db_password hello123\n",
            "\n",
            "传递团队示例:\n",
            "  rupass my_team set db_password hello123\n"
        )
    )]
    Set(TeamScopedSetArgs),
    #[command(
        name = "del",
        about = "删除密钥",
        after_help = concat!(
            "默认团队示例（本地仅有一个团队时）:\n",
            "  rupass del db_password\n",
            "\n",
            "传递团队示例:\n",
            "  rupass my_team del db_password\n"
        )
    )]
    Del(TeamScopedDeleteArgs),
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
    matches!(first_arg, "list" | "get" | "set" | "del")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_team_scoped_get_command() {
        let cli = parse_from(["rupass", "my_team", "get", "db_password"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team.as_deref(), Some("my_team"));
                match team_cli.command {
                    TeamScopedCommands::Get(args) => assert_eq!(args.key, "db_password"),
                    other => panic!("unexpected team command: {other:?}"),
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
                    other => panic!("unexpected team command: {other:?}"),
                }
            }
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_implicit_team_scoped_set_command() {
        let cli = parse_from(["rupass", "set", "db_password", "hello123"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team, None);
                match team_cli.command {
                    TeamScopedCommands::Set(args) => {
                        assert_eq!(args.key, "db_password");
                        assert_eq!(args.value, "hello123");
                    }
                    other => panic!("unexpected team command: {other:?}"),
                }
            }
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_explicit_team_scoped_list_command() {
        let cli = parse_from(["rupass", "my_team", "list"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team.as_deref(), Some("my_team"));
                match team_cli.command {
                    TeamScopedCommands::List(_) => {}
                    other => panic!("unexpected team command: {other:?}"),
                }
            }
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_team_create_command() {
        let cli = parse_from([
            "rupass",
            "team",
            "create",
            "my_team",
            "--password",
            "secret",
        ])
        .unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Team { command } => match command {
                    TeamCommands::Create(args) => {
                        assert_eq!(args.team, "my_team");
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
    fn parses_team_import_file_command() {
        let cli = parse_from([
            "rupass",
            "team",
            "import-file",
            "./finn_team.json",
            "--password",
            "secret",
        ])
        .unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Team { command } => match command {
                    TeamCommands::ImportFile(args) => {
                        assert_eq!(args.path, "./finn_team.json");
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
    fn parses_implicit_team_scoped_del_command() {
        let cli = parse_from(["rupass", "del", "db_password"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team, None);
                match team_cli.command {
                    TeamScopedCommands::Del(args) => assert_eq!(args.key, "db_password"),
                    other => panic!("unexpected team command: {other:?}"),
                }
            }
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
