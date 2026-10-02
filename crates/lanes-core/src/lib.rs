//! Shared worktree discovery, port assignment and process lifecycle for Lanes.

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;
use sysinfo::{Pid, ProcessStatus, System};

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
    #[serde(default)]
    pub isolated_process_group: bool,
    pub command: Vec<String>,
}

impl Lane {
    pub fn url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    pub fn name(&self) -> String {
        self.branch.replace(['/', '\\'], "-")
    }

    /// The environment passed to commands launched in this lane.
    pub fn environment(&self) -> [(&'static str, String); 4] {
        lane_environment(&self.branch, self.port)
    }
}

fn lane_environment(branch: &str, port: u16) -> [(&'static str, String); 4] {
    [
        ("PORT", port.to_string()),
        ("LANE_PORT", port.to_string()),
        ("LANE", branch.replace(['/', '\\'], "-")),
        ("BASE_URL", format!("http://localhost:{port}")),
    ]
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
            .truncate(false)
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
        // The separate lock serializes all Lanes readers and writers while the
        // temporary file is atomically moved into place.
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

/// Check whether a local IPv4 loopback port can be bound right now.
/// This is a snapshot; another process can claim the port afterward.
pub fn port_available(port: u16) -> bool {
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
        isolated_process_group: false,
        command: Vec::new(),
    });
    Ok(registry.lanes.len() - 1)
}

fn refresh_idle_port(registry: &mut Registry, index: usize) -> Result<()> {
    let lane = &registry.lanes[index];
    if !process_alive(lane) && !port_available(lane.port) {
        let replacement = choose_port(registry, Some(&lane.path))?;
        registry.lanes[index].port = replacement;
    }
    Ok(())
}

pub fn ensure(worktree: &Worktree) -> Result<Lane> {
    let mut registry = RegistryGuard::open()?;
    let index = upsert(&mut registry.data, worktree)?;
    refresh_idle_port(&mut registry.data, index)?;
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

fn process_alive_in(system: &System, lane: &Lane) -> bool {
    let (Some(pid), Some(start)) = (lane.pid, lane.process_started) else {
        return false;
    };
    system.process(Pid::from_u32(pid)).is_some_and(|process| {
        process.start_time() == start
            && !matches!(
                process.status(),
                ProcessStatus::Zombie | ProcessStatus::Dead
            )
    })
}

pub fn process_alive(lane: &Lane) -> bool {
    process_alive_in(&System::new_all(), lane)
}

fn prune_missing_in(registry: &mut Registry) -> Vec<Lane> {
    let system = System::new_all();
    let mut removed = Vec::new();
    registry.lanes.retain(|lane| {
        let stale = !lane.path.is_dir() && !process_alive_in(&system, lane);
        if stale {
            removed.push(lane.clone());
        }
        !stale
    });
    removed
}

/// Release ports reserved by lanes whose worktree directory is gone.
/// Missing directories with a live tracked process are retained.
pub fn prune() -> Result<Vec<Lane>> {
    let mut registry = RegistryGuard::open()?;
    let removed = prune_missing_in(&mut registry.data);
    if !removed.is_empty() {
        registry.save()?;
    }
    Ok(removed)
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

#[cfg(unix)]
enum UnixTargets {
    Group(i32),
    Tree(Vec<(u32, u64)>),
}

#[cfg(unix)]
impl UnixTargets {
    fn for_lane(lane: &Lane) -> Self {
        let pid = lane.pid.expect("running lane has a pid");
        if lane.isolated_process_group {
            return Self::Group(pid as i32);
        }
        let system = System::new_all();
        let mut descendants = Vec::new();
        fn visit(system: &System, parent: Pid, descendants: &mut Vec<(u32, u64)>) {
            for (pid, process) in system.processes() {
                if process.parent() == Some(parent) {
                    visit(system, *pid, descendants);
                    descendants.push((pid.as_u32(), process.start_time()));
                }
            }
        }
        visit(&system, Pid::from_u32(pid), &mut descendants);
        descendants.push((
            pid,
            lane.process_started.expect("running lane has start time"),
        ));
        Self::Tree(descendants)
    }

    fn signal(&self, signal: i32) -> io::Result<()> {
        match self {
            Self::Group(pgid) => signal_unix(-*pgid, signal),
            Self::Tree(targets) => {
                let system = System::new_all();
                for (pid, started) in targets {
                    if system.process(Pid::from_u32(*pid)).is_some_and(|process| {
                        process.start_time() == *started
                            && !matches!(
                                process.status(),
                                ProcessStatus::Zombie | ProcessStatus::Dead
                            )
                    }) {
                        signal_unix(*pid as i32, signal)?;
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(unix)]
fn signal_unix(pid: i32, signal: i32) -> io::Result<()> {
    if unsafe { libc::kill(pid, signal) } == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

fn wait_until_stopped(lane: &Lane, timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    loop {
        if !process_alive(lane) && port_available(lane.port) {
            return true;
        }
        if started.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn windows_script(executable: &str, directory: &Path) -> bool {
    let path = Path::new(executable);
    if let Some(extension) = path.extension() {
        return matches!(
            extension.to_string_lossy().to_ascii_lowercase().as_str(),
            "cmd" | "bat"
        );
    }
    let directories = if path.is_absolute() {
        vec![PathBuf::new()]
    } else if path.components().count() > 1 {
        vec![directory.to_path_buf()]
    } else {
        std::iter::once(directory.to_path_buf())
            .chain(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ))
            .collect()
    };
    for directory in directories {
        let candidate = directory.join(path);
        if candidate.with_extension("exe").is_file() || candidate.with_extension("com").is_file() {
            return false;
        }
        if candidate.with_extension("cmd").is_file() || candidate.with_extension("bat").is_file() {
            return true;
        }
    }
    false
}

fn build_command(
    args: &[String],
    worktree: &Worktree,
    directory: &Path,
    port: u16,
    foreground: bool,
) -> Result<Command> {
    if args.is_empty() {
        return Err(io::Error::other("missing command").into());
    }
    #[cfg(windows)]
    let mut command = {
        if windows_script(&args[0], directory) {
            // npm and similar package managers are .cmd shims.
            let line = args
                .iter()
                .map(|arg| format!("\"{}\"", arg.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" ");
            let mut shell = Command::new("cmd");
            shell.arg("/D").arg("/S").arg("/C");
            shell.raw_arg(format!(" \"{line}\""));
            shell
        } else {
            let mut process = Command::new(&args[0]);
            process.args(&args[1..]);
            process
        }
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut process = Command::new(&args[0]);
        process.args(&args[1..]);
        process
    };
    command
        .current_dir(directory)
        .envs(lane_environment(&worktree.branch, port));
    #[cfg(unix)]
    if !foreground {
        command.process_group(0);
    }
    #[cfg(windows)]
    if !foreground {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(command)
}

pub fn launch(worktree: &Worktree, args: &[String], foreground: bool) -> Result<(Lane, Child)> {
    launch_in(worktree, args, foreground, &worktree.path)
}

/// Launch a command in a worktree lane from the specified working directory.
/// The CLI uses the invoking directory; desktop launchers can use the worktree root.
pub fn launch_in(
    worktree: &Worktree,
    args: &[String],
    foreground: bool,
    directory: &Path,
) -> Result<(Lane, Child)> {
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
    refresh_idle_port(&mut registry.data, index)?;
    let port = registry.data.lanes[index].port;
    let mut command = build_command(args, worktree, directory, port, foreground)?;
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
    lane.isolated_process_group = cfg!(unix) && !foreground;
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
        {
            let status = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .status()?;
            if !status.success() {
                return Err(io::Error::other(format!("failed to stop process {pid}")).into());
            }
        }
        #[cfg(unix)]
        {
            let targets = UnixTargets::for_lane(lane);
            targets.signal(libc::SIGTERM)?;
            if !wait_until_stopped(lane, Duration::from_secs(4)) {
                targets.signal(libc::SIGKILL)?;
            }
        }
        if !wait_until_stopped(lane, Duration::from_secs(2)) {
            return Err(io::Error::other(format!(
                "process {pid} or port {} is still active after stop",
                lane.port
            ))
            .into());
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
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard};

    static LANES_HOME_LOCK: Mutex<()> = Mutex::new(());

    struct TestHome {
        previous: Option<OsString>,
        _guard: MutexGuard<'static, ()>,
    }

    impl TestHome {
        fn new(path: &Path) -> Self {
            let guard = LANES_HOME_LOCK.lock().unwrap();
            let previous = std::env::var_os("LANES_HOME");
            std::env::set_var("LANES_HOME", path);
            Self {
                previous,
                _guard: guard,
            }
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                std::env::set_var("LANES_HOME", previous);
            } else {
                std::env::remove_var("LANES_HOME");
            }
        }
    }

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
                isolated_process_group: false,
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

        let _home = TestHome::new(&temporary.path().join("data"));
        let first = discover(&repository).unwrap();
        assert_eq!(first.len(), 2);
        assert_ne!(first[0].port, first[1].port);
        let second = discover(&repository).unwrap();
        assert_eq!(
            first.iter().map(|lane| lane.port).collect::<Vec<_>>(),
            second.iter().map(|lane| lane.port).collect::<Vec<_>>()
        );
    }

    #[test]
    fn command_uses_invoking_subdirectory() {
        let temporary = tempfile::tempdir().unwrap();
        let nested = temporary.path().join("examples").join("branch-demo");
        fs::create_dir_all(&nested).unwrap();
        let worktree = Worktree {
            path: temporary.path().to_path_buf(),
            repository: "test".into(),
            common_dir: PathBuf::new(),
            branch: "feature/test".into(),
        };
        let command = build_command(
            &["node".into(), "server.mjs".into()],
            &worktree,
            &nested,
            4317,
            true,
        )
        .unwrap();
        assert_eq!(command.get_current_dir(), Some(nested.as_path()));
    }

    #[test]
    fn stop_waits_for_port_to_be_released() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let lane = Lane {
            path: PathBuf::new(),
            repository: String::new(),
            common_dir: PathBuf::new(),
            branch: String::new(),
            port,
            pid: None,
            process_started: None,
            isolated_process_group: false,
            command: Vec::new(),
        };
        let releaser = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            drop(listener);
        });
        assert!(wait_until_stopped(&lane, Duration::from_secs(2)));
        releaser.join().unwrap();
    }

    #[test]
    fn old_registry_lanes_default_to_ungrouped() {
        let lane: Lane = serde_json::from_str(
            r#"{"path":"/tmp/shop","repository":"shop","common_dir":"/tmp/shop/.git","branch":"main","port":4300,"pid":null,"process_started":null,"command":[]}"#,
        )
        .unwrap();
        assert!(!lane.isolated_process_group);
    }

    #[test]
    fn prune_only_removes_missing_stopped_worktrees() {
        let temporary = tempfile::tempdir().unwrap();
        let missing = temporary.path().join("missing");
        let existing = temporary.path().join("existing");
        fs::create_dir(&existing).unwrap();
        let make_lane = |path: PathBuf, pid: Option<u32>, process_started: Option<u64>| Lane {
            path,
            repository: "shop".into(),
            common_dir: temporary.path().join(".git"),
            branch: "main".into(),
            port: 4300,
            pid,
            process_started,
            isolated_process_group: false,
            command: Vec::new(),
        };
        let current_pid = std::process::id();
        let current_start = process_start(current_pid).unwrap();
        let mut registry = Registry {
            lanes: vec![
                make_lane(missing.clone(), None, None),
                make_lane(existing, None, None),
                make_lane(missing, Some(current_pid), Some(current_start)),
            ],
        };
        let removed = prune_missing_in(&mut registry);
        assert_eq!(removed.len(), 1);
        assert_eq!(registry.lanes.len(), 2);
        assert!(process_alive(&registry.lanes[1]));
    }

    #[cfg(unix)]
    #[test]
    fn server_helper() {
        if std::env::var_os("LANES_TEST_SERVER").is_none() {
            return;
        }
        let port = std::env::var("PORT").unwrap().parse::<u16>().unwrap();
        let _listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        loop {
            thread::park();
        }
    }

    #[cfg(unix)]
    #[test]
    fn stop_terminates_foreground_descendant_and_keeps_port() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("shop");
        fs::create_dir(&repository).unwrap();
        run_git(&repository, &["init", "-b", "main"]);
        let worktree = detect(&repository).unwrap();
        let _home = TestHome::new(&temporary.path().join("data"));

        let test_binary = std::env::current_exe().unwrap();
        let escaped = test_binary.to_string_lossy().replace('\'', "'\"'\"'");
        let script = format!(
            "LANES_TEST_SERVER=1 '{escaped}' --exact tests::server_helper --nocapture & wait"
        );
        let (lane, mut shell) =
            launch(&worktree, &["sh".into(), "-c".into(), script], true).unwrap();
        let mut ready = false;
        for _ in 0..50 {
            if !port_available(lane.port) {
                ready = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !ready {
            let _ = shell.kill();
            let _ = shell.wait();
        }
        assert!(ready, "helper server did not bind lane port");

        stop(&worktree.path).unwrap();
        shell.wait().unwrap();
        assert!(port_available(lane.port));
        assert_eq!(ensure(&worktree).unwrap().port, lane.port);
    }

    #[cfg(windows)]
    #[test]
    fn cmd_shim_preserves_quoted_arguments_and_port() {
        let temporary = tempfile::tempdir().unwrap();
        let script = temporary.path().join("dev script.cmd");
        fs::write(&script, "@echo off\r\necho %PORT%:%~1\r\n").unwrap();
        let worktree = Worktree {
            path: temporary.path().to_path_buf(),
            repository: "test".into(),
            common_dir: PathBuf::new(),
            branch: "feature/test".into(),
        };
        let output = build_command(
            &[script.to_string_lossy().into_owned(), "hello world".into()],
            &worktree,
            &worktree.path,
            4317,
            true,
        )
        .unwrap()
        .output()
        .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "4317:hello world"
        );
    }

    #[cfg(windows)]
    #[test]
    fn cmd_shim_without_extension_resolves_from_worktree() {
        let temporary = tempfile::tempdir().unwrap();
        let script = temporary.path().join("lanes_worktree_script.cmd");
        fs::write(&script, "@echo off\r\necho %PORT%:%~1\r\n").unwrap();
        let worktree = Worktree {
            path: temporary.path().to_path_buf(),
            repository: "test".into(),
            common_dir: PathBuf::new(),
            branch: "feature/test".into(),
        };
        let output = build_command(
            &["lanes_worktree_script".into(), "hello world".into()],
            &worktree,
            &worktree.path,
            4317,
            true,
        )
        .unwrap()
        .output()
        .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "4317:hello world"
        );
    }
}
