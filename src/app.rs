use anyhow::{Context, Result, bail};

use crate::cli::{
    Commands, ParsedCli, TeamCommands, TeamCreateArgs, TeamImportFileArgs, TeamScopedCommands,
};
use crate::crypto::read_existing_password;
use crate::storage::{AppPaths, list_team_configs};
use crate::tui_ops;

#[derive(Debug)]
struct ResolvedTeam {
    name: String,
}

pub(crate) fn dispatch(cli: ParsedCli) -> Result<()> {
    let paths = AppPaths::resolve()?;
    paths.ensure_base_dirs()?;

    match cli {
        ParsedCli::Standard(cli) => match cli.command {
            Commands::Tui => crate::tui::run(paths),
            Commands::Team { command } => dispatch_team_command(&paths, command),
        },
        ParsedCli::TeamScoped(cli) => {
            let team = resolve_target_team(&paths, cli.team.as_deref())?;
            match cli.command {
                TeamScopedCommands::List(args) => {
                    list_keys(&paths, &team, args.password.as_deref())
                }
                TeamScopedCommands::Get(args) => {
                    get_secret(&paths, &team, &args.key, args.password.as_deref())
                }
                TeamScopedCommands::Set(args) => set_secret(
                    &paths,
                    &team,
                    &args.key,
                    &args.value,
                    args.password.as_deref(),
                ),
                TeamScopedCommands::Del(args) => {
                    delete_secret(&paths, &team, &args.key, args.password.as_deref())
                }
            }
        }
    }
}

fn dispatch_team_command(paths: &AppPaths, command: TeamCommands) -> Result<()> {
    match command {
        TeamCommands::List => list_teams(paths),
        TeamCommands::Create(args) => create_team(paths, args),
        TeamCommands::ImportFile(args) => import_team_file(paths, args),
        TeamCommands::Del(args) => delete_team(paths, &args.team, args.password.as_deref()),
    }
}

fn resolve_target_team(paths: &AppPaths, explicit_team: Option<&str>) -> Result<ResolvedTeam> {
    resolve_target_team_with(paths, explicit_team, || {
        bail!("no team initialized. run `rupass tui` first")
    })
}

fn resolve_target_team_with<F>(
    paths: &AppPaths,
    explicit_team: Option<&str>,
    on_missing_team: F,
) -> Result<ResolvedTeam>
where
    F: FnOnce() -> Result<ResolvedTeam>,
{
    if let Some(team) = explicit_team {
        return Ok(ResolvedTeam {
            name: team.to_string(),
        });
    }

    let configs = list_team_configs(paths)?;
    match configs.as_slice() {
        [] => on_missing_team(),
        [config] => Ok(ResolvedTeam {
            name: config.team_name.clone(),
        }),
        _ => {
            let names = configs
                .iter()
                .map(|config| config.team_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("multiple teams found: {names}. please specify a team explicitly")
        }
    }
}

fn list_teams(paths: &AppPaths) -> Result<()> {
    let teams = list_team_configs(paths)?;
    if teams.is_empty() {
        println!("no teams");
        return Ok(());
    }

    for team in teams {
        println!("{}", team.team_name);
    }
    Ok(())
}

fn create_team(paths: &AppPaths, args: TeamCreateArgs) -> Result<()> {
    let (password, password_confirm) = resolve_create_passwords(&args)?;
    tui_ops::create_team(paths, &args.team, &password, &password_confirm)?;
    println!("created team: {}", args.team);
    Ok(())
}

fn delete_team(paths: &AppPaths, team: &str, password: Option<&str>) -> Result<()> {
    let password = resolve_existing_password(team, password)?;
    tui_ops::delete_team(paths, team, &password)?;
    println!("deleted team: {team}");
    Ok(())
}

fn import_team_file(paths: &AppPaths, args: TeamImportFileArgs) -> Result<()> {
    let password = match args.password {
        Some(password) => password,
        None => read_existing_password("import")?,
    };
    let team = tui_ops::import_team_file(paths, &args.path, &password)?;
    println!("imported team: {team}");
    Ok(())
}

fn list_keys(paths: &AppPaths, team: &ResolvedTeam, password: Option<&str>) -> Result<()> {
    let access = tui_ops::open_team(paths, &team.name, password)?;
    let keys = tui_ops::list_keys(paths, &access)?;
    for key in keys {
        println!("{key}");
    }
    Ok(())
}

fn get_secret(
    paths: &AppPaths,
    team: &ResolvedTeam,
    key: &str,
    password: Option<&str>,
) -> Result<()> {
    let value = if let Some(password) = password {
        let access = tui_ops::open_team(paths, &team.name, Some(password))?;
        tui_ops::get_secret_with_access(paths, &access, key)?
    } else {
        tui_ops::get_secret(paths, &team.name, key)?
    };
    println!("{value}");
    Ok(())
}

fn set_secret(
    paths: &AppPaths,
    team: &ResolvedTeam,
    key: &str,
    value: &str,
    password: Option<&str>,
) -> Result<()> {
    let access = tui_ops::open_team(paths, &team.name, password)?;
    tui_ops::set_secret(paths, &access, key, value)?;
    println!("saved key: {key}");
    Ok(())
}

fn delete_secret(
    paths: &AppPaths,
    team: &ResolvedTeam,
    key: &str,
    password: Option<&str>,
) -> Result<()> {
    let access = tui_ops::open_team(paths, &team.name, password)?;
    tui_ops::delete_secret(paths, &access, key)?;
    println!("deleted key: {key}");
    Ok(())
}

fn resolve_create_passwords(args: &TeamCreateArgs) -> Result<(String, String)> {
    match (&args.password, &args.password_confirm) {
        (Some(password), Some(confirm)) => Ok((password.clone(), confirm.clone())),
        (Some(password), None) => Ok((password.clone(), password.clone())),
        (None, Some(_)) => bail!("--password-confirm requires --password"),
        (None, None) => {
            let password = rpassword::prompt_password(format!("password for {}: ", args.team))
                .context("failed to read password")?;
            if password.is_empty() {
                bail!("password cannot be empty");
            }
            let confirm =
                rpassword::prompt_password(format!("confirm password for {}: ", args.team))
                    .context("failed to read password confirmation")?;
            Ok((password, confirm))
        }
    }
}

fn resolve_existing_password(team: &str, password: Option<&str>) -> Result<String> {
    match password {
        Some(password) => Ok(password.to_string()),
        None => read_existing_password(team),
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
