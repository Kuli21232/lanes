use lanes_core::{all_lanes, detect, discover, finish, launch, process_alive, stop, Result};
use std::env;

fn usage() {
    println!("Lanes — Run every branch. No port collisions.\n\n  lanes run [command...]  Run a command (default: npm run dev)\n  lanes status            Show worktree URLs and running state\n  lanes stop              Stop the command in the current worktree\n  lanes help              Show this help");
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
