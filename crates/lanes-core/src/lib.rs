//! Shared worktree discovery, port assignment and process lifecycle for Lanes.

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;
use sysinfo::{Pid, System};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const FIRST_PORT: u16 = 4300;
const LAST_PORT: u16 = 4999;

#[derive(Clone, Debug)]
pub struct Worktree {
    pub path: PathBuf,
    pub repository: String,
    pub common_dir: PathBuf,
    pub branch: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lane {
    pub path: PathBuf,
    pub repository: String,
    pub common_dir: PathBuf,
    pub branch: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub process_started: Option<u64>,
    pub command: Vec<String>,
}

impl Lane {
    pub fn url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    pub fn name(&self) -> String {
        self.branch.replace('/', "-").replace('\\', "-")
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Registry {
    lanes: Vec<Lane>,
}

struct RegistryGuard {
    _lock: File,
    path: PathBuf,
    data: Registry,
}

impl RegistryGuard {
    fn open() -> Result<Self> {
        let home = data_dir()?;
        fs::create_dir_all(&home)?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(home.join("registry.lock"))?;
        lock.lock_exclusive()?;
        let path = home.join("registry.json");
        let data = if path.exists() {
            let mut text = String::new();
            File::open(&path)?.read_to_string(&mut text)?;
            serde_json::from_str(&text)?
        } else {
            Registry::default()
        };
        Ok(Self {
            _lock: lock,
            path,
            data,
        })
    }

    fn save(&self) -> Result<()> {
        let temporary = self.path.with_extension("json.tmp");
        let mut file = File::create(&temporary)?;
        serde_json::to_writer_pretty(&mut file, &self.data)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        // Windows cannot rename over an existing file. The separate lock keeps
        // all Lanes readers and writers out of the short replacement window.
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

pub fn data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("LANES_HOME") {
        return Ok(PathBuf::from(path));
    }
    let base = dirs::data_local_dir()
        .ok_or_else(|| io::Error::other("cannot find user data directory"))?;
    Ok(base.join("Lanes"))
}

fn git(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

pub fn detect(path: &Path) -> Result<Worktree> {
    let root = PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?).canonicalize()?;
    let common_dir = PathBuf::from(git(
        &root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?)
    .canonicalize()?;
    let branch = git(&root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .or_else(|_| git(&root, &["rev-parse", "--short", "HEAD"]))?;
    let repository = common_dir
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            root.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    Ok(Worktree {
        path: root,
        repository,
        common_dir,
        branch,
    })
}

pub fn worktrees(path: &Path) -> Result<Vec<Worktree>> {
    let output = git(path, &["worktree", "list", "--porcelain"])?;
    let mut found = Vec::new();
    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Ok(worktree) = detect(Path::new(path)) {
                found.push(worktree);
            }
        }
    }
    Ok(found)
}

fn same_path(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn choose_port(registry: &Registry, current: Option<&Path>) -> Result<u16> {
    let reserved: HashSet<u16> = registry
        .lanes
        .iter()
        .filter(|lane| current.is_none_or(|path| !same_path(&lane.path, path)))
        .map(|lane| lane.port)
        .collect();
    (FIRST_PORT..=LAST_PORT)
        .find(|port| !reserved.contains(port) && port_available(*port))
        .ok_or_else(|| io::Error::other("no free port in 4300-4999").into())
}

fn upsert(registry: &mut Registry, worktree: &Worktree) -> Result<usize> {
    if let Some(index) = registry
        .lanes
        .iter()
        .position(|lane| same_path(&lane.path, &worktree.path))
    {
        let lane = &mut registry.lanes[index];
        lane.branch = worktree.branch.clone();
        lane.repository = worktree.repository.clone();
        lane.common_dir = worktree.common_dir.clone();
        return Ok(index);
    }
    let port = choose_port(registry, None)?;
    registry.lanes.push(Lane {
        path: worktree.path.clone(),
        repository: worktree.repository.clone(),
        common_dir: worktree.common_dir.clone(),
        branch: worktree.branch.clone(),
        port,
        pid: None,
        process_started: None,
        command: Vec::new(),
    });
    Ok(registry.lanes.len() - 1)
}

pub fn ensure(worktree: &Worktree) -> Result<Lane> {
    let mut registry = RegistryGuard::open()?;
    let index = upsert(&mut registry.data, worktree)?;
    registry.save()?;
    Ok(registry.data.lanes[index].clone())
}

pub fn discover(path: &Path) -> Result<Vec<Lane>> {
    let trees = worktrees(path)?;
    let mut registry = RegistryGuard::open()?;
    let mut result = Vec::new();
    for tree in trees {
        let index = upsert(&mut registry.data, &tree)?;
        result.push(registry.data.lanes[index].clone());
    }
    registry.save()?;
    Ok(result)
}

pub fn all_lanes() -> Result<Vec<Lane>> {
    Ok(RegistryGuard::open()?.data.lanes)
}

pub fn process_alive(lane: &Lane) -> bool {
    let (Some(pid), Some(start)) = (lane.pid, lane.process_started) else {
        return false;
    };
    let system = System::new_all();
    system
        .process(Pid::from_u32(pid))
        .is_some_and(|process| process.start_time() == start)
}

fn process_start(pid: u32) -> Option<u64> {
    for _ in 0..10 {
        let system = System::new_all();
        if let Some(process) = system.process(Pid::from_u32(pid)) {
            return Some(process.start_time());
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

fn build_command(args: &[String], worktree: &Worktree, port: u16) -> Result<Command> {
    if args.is_empty() {
        return Err(io::Error::other("missing command").into());
    }
    #[cfg(windows)]
    let mut command = {
        // npm, pnpm and other Windows package managers are .cmd shims.
        let mut shell = Command::new("cmd");
        shell.arg("/D").arg("/S").arg("/C");
        let line = args
            .iter()
            .map(|arg| {
                if arg.contains([' ', '\t', '"']) {
                    format!("\"{}\"", arg.replace('"', "\"\""))
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        shell.arg(line);
        shell
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut process = Command::new(&args[0]);
        process.args(&args[1..]);
        process
    };
    command
        .current_dir(&worktree.path)
        .env("PORT", port.to_string())
        .env("LANE_PORT", port.to_string())
        .env("LANE", worktree.branch.replace('/', "-"))
        .env("BASE_URL", format!("http://localhost:{port}"));
    Ok(command)
}

pub fn launch(worktree: &Worktree, args: &[String], foreground: bool) -> Result<(Lane, Child)> {
    let mut registry = RegistryGuard::open()?;
    let index = upsert(&mut registry.data, worktree)?;
    let old = &registry.data.lanes[index];
    if process_alive(old) {
        return Err(io::Error::other(format!(
            "{} is already running at {}",
            old.branch,
            old.url()
        ))
        .into());
    }
    if !port_available(old.port) {
        let replacement = choose_port(&registry.data, Some(&worktree.path))?;
        registry.data.lanes[index].port = replacement;
    }
    let port = registry.data.lanes[index].port;
    let mut command = build_command(args, worktree, port)?;
    if foreground {
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    } else {
        let log = data_dir()?.join(format!("lane-{port}.log"));
        let output = File::create(log)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::from(output));
    }
    let child = command.spawn()?;
    let lane = &mut registry.data.lanes[index];
    lane.pid = Some(child.id());
    lane.process_started = process_start(child.id());
    lane.command = args.to_vec();
    registry.save()?;
    Ok((registry.data.lanes[index].clone(), child))
}

pub fn finish(path: &Path, pid: u32) -> Result<()> {
    let mut registry = RegistryGuard::open()?;
    if let Some(lane) = registry
        .data
        .lanes
        .iter_mut()
        .find(|lane| same_path(&lane.path, path))
    {
        if lane.pid == Some(pid) {
            lane.pid = None;
            lane.process_started = None;
            registry.save()?;
        }
    }
    Ok(())
}

pub fn stop(path: &Path) -> Result<Lane> {
    let mut registry = RegistryGuard::open()?;
    let lane = registry
        .data
        .lanes
        .iter_mut()
        .find(|lane| same_path(&lane.path, path))
        .ok_or_else(|| io::Error::other("worktree has no lane"))?;
    if process_alive(lane) {
        let pid = lane.pid.expect("live lane has a pid");
        #[cfg(windows)]
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()?;
        #[cfg(not(windows))]
        let status = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!("failed to stop process {pid}")).into());
        }
    }
    lane.pid = None;
    lane.process_started = None;
    let result = lane.clone();
    registry.save()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_git(path: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn allocation_skips_reserved_and_busy_ports() {
        let listener = TcpListener::bind(("127.0.0.1", FIRST_PORT)).unwrap();
        let registry = Registry {
            lanes: vec![Lane {
                path: PathBuf::from("reserved"),
                repository: String::new(),
                common_dir: PathBuf::new(),
                branch: String::new(),
                port: FIRST_PORT + 1,
                pid: None,
                process_started: None,
                command: vec![],
            }],
        };
        assert_eq!(choose_port(&registry, None).unwrap(), FIRST_PORT + 2);
        drop(listener);
    }

    #[test]
    fn two_worktrees_get_distinct_stable_ports() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("shop");
        let feature = temporary.path().join("shop-auth");
        fs::create_dir(&repository).unwrap();
        run_git(&repository, &["init", "-b", "main"]);
        fs::write(repository.join("README.md"), "shop").unwrap();
        run_git(&repository, &["add", "."]);
        run_git(
            &repository,
            &[
                "-c",
                "user.name=Lanes Test",
                "-c",
                "user.email=lanes@example.com",
                "commit",
                "-m",
                "initial",
            ],
        );
        run_git(
            &repository,
            &[
                "worktree",
                "add",
                "-b",
                "feature/auth",
                feature.to_str().unwrap(),
            ],
        );

        std::env::set_var("LANES_HOME", temporary.path().join("data"));
        let first = discover(&repository).unwrap();
        assert_eq!(first.len(), 2);
        assert_ne!(first[0].port, first[1].port);
        let second = discover(&repository).unwrap();
        assert_eq!(
            first.iter().map(|lane| lane.port).collect::<Vec<_>>(),
            second.iter().map(|lane| lane.port).collect::<Vec<_>>()
        );
        std::env::remove_var("LANES_HOME");
    }
}
