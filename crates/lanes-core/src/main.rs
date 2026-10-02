use lanes_core::{
    all_lanes, detect, discover, ensure, finish, launch, process_alive, stop, Lane, Result,
};
use std::env;
use std::io;

fn usage() {
    println!("Lanes — Run every branch. No port collisions.\n\n  lanes run [command...]       Run a command (default: npm run dev)\n  lanes status                 Show worktree URLs and running state\n  lanes env [--shell FORMAT]   Print environment (dotenv, sh, powershell)\n  lanes stop                   Stop the command in the current worktree\n  lanes help                   Show this help");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EnvFormat {
    Dotenv,
    Sh,
    PowerShell,
}

fn parse_env_format(args: &[String]) -> Result<EnvFormat> {
    match args {
        [] => Ok(EnvFormat::Dotenv),
        [flag, format] if flag == "--shell" => match format.as_str() {
            "dotenv" => Ok(EnvFormat::Dotenv),
            "sh" | "bash" | "zsh" => Ok(EnvFormat::Sh),
            "powershell" | "pwsh" => Ok(EnvFormat::PowerShell),
            _ => Err(io::Error::other(format!("unsupported shell: {format}")).into()),
        },
        _ => Err(io::Error::other("usage: lanes env [--shell dotenv|sh|powershell]").into()),
    }
}

fn format_environment(lane: &Lane, format: EnvFormat) -> String {
    lane.environment()
        .into_iter()
        .map(|(name, value)| match format {
            EnvFormat::Dotenv => format!("{name}={}", serde_json::to_string(&value).unwrap()),
            EnvFormat::Sh => format!("export {name}='{}'", value.replace('\'', "'\"'\"'")),
            EnvFormat::PowerShell => format!("$env:{name} = '{}'", value.replace('\'', "''")),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn real_main() -> Result<i32> {
    let mut args = env::args().skip(1);
    let action = args.next().unwrap_or_else(|| "status".into());
    let cwd = env::current_dir()?;
    match action.as_str() {
        "run" => {
            let worktree = detect(&cwd)?;
            let mut command: Vec<String> = args.collect();
            if command.first().is_some_and(|value| value == "--") {
                command.remove(0);
            }
            if command.is_empty() {
                command = vec!["npm".into(), "run".into(), "dev".into()];
            }
            let (lane, mut child) = launch(&worktree, &command, true)?;
            println!(
                "\nLANES  {} · {}\nPORT={}  {}\n",
                lane.repository,
                lane.branch,
                lane.port,
                lane.url()
            );
            let code = child.wait()?.code().unwrap_or(1);
            finish(&lane.path, child.id())?;
            Ok(code)
        }
        "status" => {
            let lanes = if detect(&cwd).is_ok() {
                discover(&cwd)?
            } else {
                all_lanes()?
            };
            if lanes.is_empty() {
                println!("No lanes yet. Run `lanes run` in a Git worktree.");
            } else {
                println!("LANES\n");
                for lane in &lanes {
                    println!(
                        "{:<24} {:<23} {}",
                        lane.branch,
                        lane.url(),
                        if process_alive(lane) {
                            "● running"
                        } else {
                            "○ stopped"
                        }
                    );
                }
                println!(
                    "\n{} worktrees · {} running",
                    lanes.len(),
                    lanes.iter().filter(|lane| process_alive(lane)).count()
                );
            }
            Ok(0)
        }
        "env" => {
            let format = parse_env_format(&args.collect::<Vec<_>>())?;
            let lane = ensure(&detect(&cwd)?)?;
            println!("{}", format_environment(&lane, format));
            Ok(0)
        }
        "stop" => {
            let lane = stop(&detect(&cwd)?.path)?;
            println!("Stopped {} ({})", lane.branch, lane.url());
            Ok(0)
        }
        "help" | "--help" | "-h" => {
            usage();
            Ok(0)
        }
        _ => {
            usage();
            Err(std::io::Error::other(format!("unknown command: {action}")).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn lane(branch: &str) -> Lane {
        Lane {
            path: PathBuf::new(),
            repository: "shop".into(),
            common_dir: PathBuf::new(),
            branch: branch.into(),
            port: 4317,
            pid: None,
            process_started: None,
            command: Vec::new(),
        }
    }

    #[test]
    fn environment_formats_match_run_variables_and_escape_quotes() {
        let lane = lane("feature/o'hare");
        assert_eq!(
            format_environment(&lane, EnvFormat::Dotenv),
            "PORT=\"4317\"\nLANE_PORT=\"4317\"\nLANE=\"feature-o'hare\"\nBASE_URL=\"http://localhost:4317\""
        );
        assert!(
            format_environment(&lane, EnvFormat::Sh).contains("export LANE='feature-o'\"'\"'hare'")
        );
        assert!(format_environment(&lane, EnvFormat::PowerShell)
            .contains("$env:LANE = 'feature-o''hare'"));
    }

    #[test]
    fn env_format_rejects_unrecognized_arguments() {
        assert_eq!(parse_env_format(&[]).unwrap(), EnvFormat::Dotenv);
        assert_eq!(
            parse_env_format(&["--shell".into(), "pwsh".into()]).unwrap(),
            EnvFormat::PowerShell
        );
        assert!(parse_env_format(&["--shell".into()]).is_err());
        assert!(parse_env_format(&["--shell".into(), "fish".into()]).is_err());
    }
}

fn main() {
    match real_main() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("lanes: {error}");
            std::process::exit(1);
        }
    }
}
