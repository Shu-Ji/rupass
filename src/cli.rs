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
    override_usage = "rupass <COMMAND>\n       rupass <team> <COMMAND>",
    long_about = "rupass 是一个轻量级团队密码管理工具。\n支持按团队隔离存储、团队级加密、独立 git 仓库和自动同步。",
    after_help = "命令示例:\n  通用:\n    rupass init this_is_a_test_team --display-name 测试团队\n    rupass team ls\n    rupass team delete this_is_a_test_team\n    rupass sync-all\n\n  传入团队:\n    rupass this_is_a_test_team ls\n    rupass this_is_a_test_team set db_password my-secret\n    rupass this_is_a_test_team get db_password\n    rupass this_is_a_test_team del db_password\n    rupass this_is_a_test_team set-remote --url git@github.com:org/this-is-a-test-team.git\n    rupass this_is_a_test_team sync\n\n  不传团队，使用默认 default_team:\n    rupass ls\n    rupass set db_password my-secret\n    rupass get db_password\n    rupass del db_password\n    rupass set-remote --url git@github.com:org/default-team.git\n    rupass sync\n\n团队说明:\n  所有密钥都必须属于某个团队。\n  如果本地没有团队，首次省略 team 时会自动创建 `default_team`。\n  如果本地只有一个团队，密钥相关操作可省略 team；如果有多个团队，必须显式传入。\n  `rupass ls/list` 用于列当前团队 key；`rupass team ls/list` 用于列所有团队。\n\n密码输入:\n  `get` 默认不需要密码。\n  创建团队和其他 team 相关操作需要密码。"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    #[command(
        about = "初始化团队",
        long_about = "初始化一个团队，创建配置文件和团队存储目录。\n团队名必须显式指定，且必须以 _team 结尾。",
        after_help = "示例:\n  rupass init this_is_a_test_team --display-name 测试团队\n  rupass init default_team"
    )]
    Init(InitArgs),
    #[command(
        name = "sync-all",
        about = "同步所有团队仓库",
        long_about = "遍历所有已初始化团队，并逐个执行同步。",
        after_help = "示例:\n  rupass sync-all"
    )]
    SyncAll,
    #[command(
        about = "团队管理",
        long_about = "用于管理团队信息，包括查看团队列表和删除团队。",
        after_help = "示例:\n  rupass team list\n  rupass team delete this_is_a_test_team"
    )]
    Team {
        #[command(subcommand)]
        command: TeamCommands,
    },
}

#[derive(Parser, Debug)]
#[command(
    name = "rupass",
    version,
    about = "团队作用域命令",
    long_about = "使用 `rupass <team> <command>` 在指定团队下执行密钥和团队操作。若本地只有一个团队，也可直接使用 `rupass <command>`。",
    after_help = "示例:\n  rupass this_is_a_test_team ls\n  rupass this_is_a_test_team set db_password my-secret\n  rupass get db_password\n  rupass del db_password"
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
    about = "默认团队作用域命令",
    long_about = "当本地只有一个团队时，可省略 team 直接执行团队相关操作。",
    after_help = "示例:\n  rupass ls\n  rupass get db_password\n  rupass set api_token token-123"
)]
pub(crate) struct ImplicitTeamScopedCli {
    #[command(subcommand)]
    pub(crate) command: TeamScopedCommands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum TeamScopedCommands {
    #[command(
        name = "ls",
        alias = "list",
        about = "列出当前团队下所有 key",
        long_about = "列出当前团队内保存的所有 key 名称。",
        after_help = "示例:\n  rupass this_is_a_test_team ls"
    )]
    List,
    #[command(
        about = "新增或更新密钥",
        long_about = "保存一个 key/value 到当前团队。",
        after_help = "示例:\n  rupass this_is_a_test_team set db_password my-secret"
    )]
    Set(TeamScopedSecretSetArgs),
    #[command(
        about = "读取密钥值",
        long_about = "按 key 读取当前团队中的密钥值。",
        after_help = "示例:\n  rupass this_is_a_test_team get db_password"
    )]
    Get(TeamScopedSecretKeyArgs),
    #[command(
        name = "del",
        alias = "delete",
        alias = "rm",
        alias = "remove",
        about = "删除密钥",
        long_about = "删除当前团队中的一个 key。",
        after_help = "示例:\n  rupass this_is_a_test_team del db_password"
    )]
    Delete(TeamScopedSecretKeyArgs),
    #[command(
        about = "设置当前团队 git 远程地址",
        long_about = "为当前团队配置 git 远程仓库地址。",
        after_help = "示例:\n  rupass this_is_a_test_team set-remote --url git@github.com:org/this-is-a-test-team.git"
    )]
    SetRemote(TeamScopedRemoteArgs),
    #[command(
        about = "手动同步当前团队仓库",
        long_about = "手动执行当前团队的 git 同步。",
        after_help = "示例:\n  rupass this_is_a_test_team sync"
    )]
    Sync,
}

#[derive(Args, Debug)]
pub(crate) struct InitArgs {
    #[arg(help = "团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
    #[arg(long, help = "团队显示名称，可使用中文；不传时默认使用团队名")]
    pub(crate) display_name: Option<String>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum TeamCommands {
    #[command(
        alias = "ls",
        about = "列出所有团队",
        long_about = "列出当前已初始化的所有团队，包括团队英文名、显示名和 git 远程地址。",
        after_help = "示例:\n  rupass team list"
    )]
    List,
    #[command(
        name = "delete",
        alias = "del",
        alias = "rm",
        alias = "remove",
        about = "删除团队",
        long_about = "删除指定团队的配置和本地存储目录。执行前需要输入该团队密码。",
        after_help = "示例:\n  rupass team delete this_is_a_test_team"
    )]
    Delete(TeamDeleteArgs),
}

#[derive(Args, Debug)]
pub(crate) struct TeamScopedSecretSetArgs {
    #[arg(help = "要保存的 key 名称")]
    pub(crate) key: String,
    #[arg(help = "要保存的 value 内容")]
    pub(crate) value: String,
}

#[derive(Args, Debug)]
pub(crate) struct TeamScopedSecretKeyArgs {
    #[arg(help = "要读取或删除的 key 名称")]
    pub(crate) key: String,
}

#[derive(Args, Debug)]
pub(crate) struct TeamScopedRemoteArgs {
    #[arg(long, help = "要设置的 git 远程地址")]
    pub(crate) url: String,
}

#[derive(Args, Debug)]
pub(crate) struct TeamDeleteArgs {
    #[arg(help = "要删除的团队英文名，必须以 _team 结尾")]
    pub(crate) team: String,
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
    matches!(
        first_arg,
        "ls" | "list" | "set" | "get" | "del" | "delete" | "rm" | "remove" | "set-remote" | "sync"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_team_scoped_set_command() {
        let cli = parse_from(["rupass", "dev_team", "set", "db_password", "secret"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team.as_deref(), Some("dev_team"));
                match team_cli.command {
                    TeamScopedCommands::Set(args) => {
                        assert_eq!(args.key, "db_password");
                        assert_eq!(args.value, "secret");
                    }
                    command => panic!("unexpected command: {command:?}"),
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
                    TeamScopedCommands::Get(args) => {
                        assert_eq!(args.key, "db_password");
                    }
                    command => panic!("unexpected command: {command:?}"),
                }
            }
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_top_level_ls_command() {
        let cli = parse_from(["rupass", "ls"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team, None);
                match team_cli.command {
                    TeamScopedCommands::List => {}
                    command => panic!("unexpected command: {command:?}"),
                }
            }
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_team_list_command() {
        let cli = parse_from(["rupass", "team", "ls"]).unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Team { command } => match command {
                    TeamCommands::List => {}
                    command => panic!("unexpected command: {command:?}"),
                },
                command => panic!("unexpected command: {command:?}"),
            },
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_init_with_positional_team() {
        let cli = parse_from(["rupass", "init", "dev_team", "--display-name", "开发团队"]).unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Init(args) => {
                    assert_eq!(args.team, "dev_team");
                    assert_eq!(args.display_name.as_deref(), Some("开发团队"));
                }
                command => panic!("unexpected command: {command:?}"),
            },
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
    fn parses_team_delete_command() {
        let cli = parse_from(["rupass", "team", "delete", "dev_team"]).unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Team { command } => match command {
                    TeamCommands::Delete(args) => assert_eq!(args.team, "dev_team"),
                    command => panic!("unexpected command: {command:?}"),
                },
                command => panic!("unexpected command: {command:?}"),
            },
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_implicit_rm_command() {
        let cli = parse_from(["rupass", "rm", "db_password"]).unwrap();
        match cli {
            ParsedCli::TeamScoped(team_cli) => {
                assert_eq!(team_cli.team, None);
                match team_cli.command {
                    TeamScopedCommands::Delete(args) => assert_eq!(args.key, "db_password"),
                    command => panic!("unexpected command: {command:?}"),
                }
            }
            other => panic!("unexpected cli: {other:?}"),
        }
    }

    #[test]
    fn parses_team_remove_command() {
        let cli = parse_from(["rupass", "team", "remove", "dev_team"]).unwrap();
        match cli {
            ParsedCli::Standard(cli) => match cli.command {
                Commands::Team { command } => match command {
                    TeamCommands::Delete(args) => assert_eq!(args.team, "dev_team"),
                    command => panic!("unexpected command: {command:?}"),
                },
                command => panic!("unexpected command: {command:?}"),
            },
            other => panic!("unexpected cli: {other:?}"),
        }
    }
}
