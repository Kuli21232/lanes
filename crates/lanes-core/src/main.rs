use lanes_core::{
    all_lanes, data_dir, detect, discover, ensure, finish, launch_in, port_available,
    process_alive, processes_alive, prune, stop, Lane, Result,
};
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn usage() {
    println!("Lanes — Run every branch. No port collisions.\n\n  lanes run [command...]       Run a command (default: npm run dev)\n  lanes status                 Show worktree URLs and running state\n  lanes env [--shell FORMAT]   Print environment (dotenv, sh, powershell)\n  lanes doctor [command...]    Check Git, lane port and optional command\n  lanes prune                  Release ports for deleted worktrees\n  lanes stop                   Stop the command in the current worktree\n  lanes help                   Show this help");
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

fn executable_at(path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(extension) = path.extension() {
            let executable = matches!(
                extension.to_string_lossy().to_ascii_lowercase().as_str(),
                "exe" | "com" | "bat" | "cmd"
            );
            return (executable && path.is_file()).then(|| path.to_path_buf());
        }
        for extension in ["exe", "com", "bat", "cmd"] {
            let candidate = path.with_extension(extension);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.is_file() && path.metadata().ok()?.permissions().mode() & 0o111 != 0 {
            return Some(path.to_path_buf());
        }
    }
    None
}

fn resolve_executable(name: &str, cwd: &Path) -> Option<PathBuf> {
    let path = Path::new(name);
    if path.is_absolute() || path.components().count() > 1 {
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        return executable_at(&candidate);
    }
    #[cfg(windows)]
    if let Some(candidate) = executable_at(&cwd.join(path)) {
        return Some(candidate);
    }
    env::split_paths(&env::var_os("PATH")?)
        .find_map(|directory| executable_at(&directory.join(path)))
}

fn doctor(cwd: &Path, command: &[String]) -> Result<i32> {
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => {
            println!("✓ {}", String::from_utf8_lossy(&output.stdout).trim());
        }
        Ok(output) => {
            eprintln!(
                "✗ Git failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            return Ok(1);
        }
        Err(error) => {
            eprintln!("✗ Git unavailable: {error}");
            return Ok(1);
        }
    }

    let worktree = match detect(cwd) {
        Ok(worktree) => worktree,
        Err(error) => {
            eprintln!("✗ Worktree detection failed: {error}");
            return Ok(1);
        }
    };
    println!(
        "✓ Worktree: {} ({})",
        worktree.branch,
        worktree.path.display()
    );

    let lane = match ensure(&worktree) {
        Ok(lane) => lane,
        Err(error) => {
            eprintln!("✗ Lane registry failed: {error}");
            return Ok(1);
        }
    };
    println!("✓ Registry: {}", data_dir()?.display());

    let running = process_alive(&lane);
    if running {
        println!("✓ Lane: {} is running on {}", lane.name(), lane.url());
    } else if port_available(lane.port) {
        println!("✓ Port {} is available for {}", lane.port, lane.name());
    } else {
        eprintln!("✗ Port {} is occupied by another process", lane.port);
        return Ok(1);
    }
    println!(
        "✓ Child environment: PORT={}, LANE={}, BASE_URL={}",
        lane.port,
        lane.name(),
        lane.url()
    );

    if let Some(program) = command.first() {
        match resolve_executable(program, cwd) {
            Some(path) => println!("✓ Command: {}", path.display()),
            None => {
                eprintln!("✗ Command not found or not executable: {program}");
                return Ok(1);
            }
        }
    }
    Ok(0)
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
            let (lane, mut child) = launch_in(&worktree, &command, true, &cwd)?;
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
                let running = processes_alive(&lanes);
                for (lane, is_running) in lanes.iter().zip(&running) {
                    println!(
                        "{:<24} {:<23} {}",
                        lane.branch,
                        lane.url(),
                        if *is_running {
                            "● running"
                        } else {
                            "○ stopped"
                        }
                    );
                }
                println!(
                    "\n{} worktrees · {} running",
                    lanes.len(),
                    running.into_iter().filter(|is_running| *is_running).count()
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
        "doctor" => {
            let mut command: Vec<String> = args.collect();
            if command.first().is_some_and(|value| value == "--") {
                command.remove(0);
            }
            doctor(&cwd, &command)
        }
        "prune" => {
            if args.next().is_some() {
                return Err(io::Error::other("usage: lanes prune").into());
            }
            let removed = prune()?;
            for lane in &removed {
                println!("Released {} ({})", lane.path.display(), lane.port);
            }
            println!("{} stale lane(s) pruned", removed.len());
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

fn main() {
    match real_main() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("lanes: {error}");
            std::process::exit(1);
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
            isolated_process_group: false,
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

    #[test]
    fn doctor_resolves_executable_without_running_it() {
        let executable = env::current_exe().unwrap();
        assert_eq!(
            resolve_executable(executable.to_str().unwrap(), Path::new(".")),
            Some(executable)
        );
        assert!(resolve_executable("/missing/lanes-doctor-tool", Path::new(".")).is_none());
    }
}
