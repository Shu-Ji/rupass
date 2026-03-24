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
    override_usage = "rupass <COMMAND>\n       rupass [team] <COMMAND>",
    long_about = "rupass 是一个轻量级团队密码管理工具。\n支持 TUI 和完整 CLI，便于脚本或 AI 通过命令行管理团队、密钥与同步。",
    after_help = concat!(
        "通用示例:\n",
        "  rupass tui\n",
        "  rupass team list\n",
        "  rupass team create my_team --password secret\n",
        "  rupass team import git@github.com:org/repo.git --password secret\n",
        "  rupass team set-remote my_team git@github.com:org/repo.git\n",
        "  rupass team set-s3 my_team --endpoint https://s3.example.com --region us-east-1 --bucket my-bucket --access-key-id AKIA... --secret-access-key xxxx --root team\n",
        "  rupass sync-all\n",
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
        name = "sync-all",
        about = "同步所有团队仓库",
        after_help = "示例:\n  rupass sync-all"
    )]
    SyncAll,
    #[command(
        name = "team",
        about = "团队管理命令",
        after_help = "示例:\n  rupass team list\n  rupass team create my_team --password secret\n  rupass team import git@github.com:org/repo.git --password secret\n  rupass team set-remote my_team git@github.com:org/repo.git\n  rupass team set-s3 my_team --endpoint https://s3.example.com --region us-east-1 --bucket my-bucket --access-key-id AKIA... --secret-access-key xxxx --root team"
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
    #[command(about = "从远程仓库导入团队")]
    Import(TeamImportArgs),
    #[command(name = "del", about = "删除团队")]
    Del(TeamPasswordTargetArgs),
    #[command(about = "设置团队远程仓库")]
    SetRemote(TeamSetRemoteArgs),
    #[command(about = "清空团队远程仓库")]
    ClearRemote(TeamPasswordTargetArgs),
    #[command(about = "设置团队 S3 远程")]
    SetS3(TeamSetS3Args),
    #[command(about = "清空团队 S3 远程")]
    ClearS3(TeamClearS3Args),
    #[command(about = "同步指定团队")]
    Sync(TeamPasswordTargetArgs),
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
pub(crate) struct TeamImportArgs {
    #[arg(help = "远程仓库地址；兼容旧格式时也可先传团队名", required = true, num_args = 1..=2)]
    pub(crate) args: Vec<String>,
    #[arg(long, help = "本地团队名；不传则从远程仓库元数据读取")]
    pub(crate) team: Option<String>,
    #[arg(long, help = "团队密码；不传则交互输入")]
    pub(crate) password: Option<String>,
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
pub(crate) struct TeamSetS3Args {
    #[arg(help = "团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
    #[arg(long, help = "S3 endpoint，例如 https://s3.example.com")]
    pub(crate) endpoint: String,
    #[arg(long, help = "S3 region，例如 us-east-1")]
    pub(crate) region: String,
    #[arg(long, help = "S3 bucket")]
    pub(crate) bucket: String,
    #[arg(long = "access-key-id", help = "S3 access key id")]
    pub(crate) access_key_id: String,
    #[arg(long = "secret-access-key", help = "S3 secret access key")]
    pub(crate) secret_access_key: String,
    #[arg(long, help = "S3 root prefix，可选")]
    pub(crate) root: Option<String>,
    #[arg(long, default_value_t = true, help = "是否使用 path-style 访问")]
    pub(crate) force_path_style: bool,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
    pub(crate) password: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct TeamClearS3Args {
    #[arg(help = "团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
    #[arg(long, help = "团队密码；不传时会尝试使用已缓存密钥，必要时再交互输入")]
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
    fn parses_team_import_command_without_team() {
        let cli = parse_from([
            "rupass",
            "team",
            "import",
            "git@github.com:org/repo.git",
            "--password",
            "secret",
        ])
        .unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Team { command } => match command {
                    TeamCommands::Import(args) => {
                        assert_eq!(args.args, vec!["git@github.com:org/repo.git"]);
                        assert_eq!(args.team, None);
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
    fn parses_team_import_command_with_legacy_team_arg() {
        let cli = parse_from([
            "rupass",
            "team",
            "import",
            "my_team",
            "git@github.com:org/repo.git",
        ])
        .unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Team { command } => match command {
                    TeamCommands::Import(args) => {
                        assert_eq!(args.args, vec!["my_team", "git@github.com:org/repo.git"]);
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
