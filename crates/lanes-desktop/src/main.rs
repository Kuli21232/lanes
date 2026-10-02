#![cfg_attr(windows, windows_subsystem = "windows")]

slint::include_modules!();

use lanes_core::{data_dir, detect, discover, finish, launch, process_alive, stop, Lane, Result};
use slint::{ModelRc, Timer, TimerMode, VecModel};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

const LOG_TAIL_BYTES: u64 = 64 * 1024;
const LAST_PROJECT_FILE: &str = "desktop-project.txt";

fn remembered_project() -> Option<PathBuf> {
    let path = PathBuf::from(fs::read_to_string(data_dir().ok()?.join(LAST_PROJECT_FILE)).ok()?);
    detect(&path).ok().map(|_| path)
}

fn initial_project() -> Option<PathBuf> {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .filter(|path| detect(path).is_ok())
        })
        .or_else(remembered_project)
}

fn remember_project(path: &Path) -> Result<()> {
    let home = data_dir()?;
    fs::create_dir_all(&home)?;
    fs::write(
        home.join(LAST_PROJECT_FILE),
        path.to_string_lossy().as_bytes(),
    )?;
    Ok(())
}

fn refresh(ui: &AppWindow, announce: bool) {
    let path = ui.get_project_path().to_string();
    if path.trim().is_empty() {
        ui.set_lanes(ModelRc::new(VecModel::default()));
        ui.set_summary("No worktrees loaded".into());
        ui.set_message("Choose a Git repository to get started".into());
        ui.set_has_error(false);
        return;
    }
    match discover(Path::new(&path)) {
        Ok(lanes) => {
            let mut running = 0;
            let rows = lanes
                .iter()
                .map(|lane| {
                    let is_running = process_alive(lane);
                    running += usize::from(is_running);
                    LaneRow {
                        branch: lane.branch.clone().into(),
                        url: lane.url().into(),
                        running: is_running,
                        path: lane.path.to_string_lossy().into_owned().into(),
                        port: lane.port.into(),
                    }
                })
                .collect::<Vec<_>>();
            ui.set_lanes(ModelRc::new(VecModel::from(rows)));
            ui.set_summary(format!("{} worktrees  ·  {running} running", lanes.len()).into());
            if announce {
                match remember_project(Path::new(&path)) {
                    Ok(()) => {
                        ui.set_message("Worktrees refreshed".into());
                        ui.set_has_error(false);
                    }
                    Err(error) => {
                        ui.set_message(
                            format!("Worktrees loaded, but folder was not saved: {error}").into(),
                        );
                        ui.set_has_error(true);
                    }
                }
            } else if ui.get_message().starts_with("Could not load worktrees:") {
                ui.set_message("Ready".into());
                ui.set_has_error(false);
            }
        }
        Err(error) => {
            ui.set_lanes(ModelRc::new(VecModel::default()));
            ui.set_summary("No worktrees loaded".into());
            ui.set_message(format!("Could not load worktrees: {error}").into());
            ui.set_has_error(true);
        }
    }
}

fn run_lane(path: &str, command: &str) -> Result<Lane> {
    let worktree = detect(Path::new(path))?;
    let args = split_command(command)?;
    let (lane, mut child) = launch(&worktree, &args, false)?;
    let pid = child.id();
    let path = lane.path.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        let _ = finish(&path, pid);
    });
    Ok(lane)
}

#[cfg(not(windows))]
fn split_command(command: &str) -> Result<Vec<String>> {
    Ok(shell_words::split(command)?)
}

#[cfg(windows)]
fn split_command(command: &str) -> Result<Vec<String>> {
    use std::io;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

    if command.trim().is_empty() {
        return Ok(Vec::new());
    }
    if command.contains('\0') {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "command contains a NUL byte").into(),
        );
    }
    let mut wide: Vec<u16> = command.trim().encode_utf16().chain(Some(0)).collect();
    let mut count = 0;
    // SAFETY: `wide` is NUL-terminated and stays alive until the call returns.
    let argv = unsafe { CommandLineToArgvW(wide.as_mut_ptr(), &mut count) };
    if argv.is_null() {
        return Err(io::Error::last_os_error().into());
    }
    let mut result = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        // SAFETY: CommandLineToArgvW returned an array of `count` NUL-terminated strings.
        let argument = unsafe { *argv.add(index) };
        let mut length = 0;
        // SAFETY: each argument points to a NUL-terminated UTF-16 string in the returned block.
        while unsafe { *argument.add(length) } != 0 {
            length += 1;
        }
        // SAFETY: `length` was found before the string's terminating NUL.
        let slice = unsafe { std::slice::from_raw_parts(argument, length) };
        result.push(String::from_utf16_lossy(slice));
    }
    // SAFETY: CommandLineToArgvW allocates this block with LocalAlloc.
    unsafe { LocalFree(argv.cast()) };
    Ok(result)
}

fn read_log(port: u16) -> Result<String> {
    let path = data_dir()?.join(format!("lane-{port}.log"));
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok("No log yet. Run this worktree to create one.".into())
        }
        Err(error) => return Err(error.into()),
    };
    let length = file.metadata()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(LOG_TAIL_BYTES)))?;
    let mut bytes = Vec::new();
    (&mut file).take(LOG_TAIL_BYTES).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn refresh_log(ui: &AppWindow) {
    let port = match u16::try_from(ui.get_log_port()) {
        Ok(port) => port,
        Err(_) => return,
    };
    match read_log(port) {
        Ok(contents) => ui.set_log_text(contents.into()),
        Err(error) => ui.set_log_text(format!("Could not read log: {error}").into()),
    }
}

fn main() -> Result<()> {
    let ui = AppWindow::new()?;
    if let Some(path) = initial_project() {
        ui.set_project_path(path.to_string_lossy().into_owned().into());
    }

    let weak = ui.as_weak();
    ui.on_refresh(move || {
        if let Some(ui) = weak.upgrade() {
            refresh(&ui, true);
        }
    });

    let weak = ui.as_weak();
    ui.on_choose_project(move || {
        if let Some(ui) = weak.upgrade() {
            let mut dialog = rfd::FileDialog::new().set_title("Select a Git worktree");
            #[cfg(windows)]
            if let Some(home) = std::env::var_os("USERPROFILE") {
                if Path::new(&home).is_dir() {
                    dialog = dialog.set_directory(home);
                }
            }
            if let Some(path) = dialog.pick_folder() {
                ui.set_project_path(path.to_string_lossy().into_owned().into());
                refresh(&ui, true);
            }
        }
    });

    let weak = ui.as_weak();
    ui.on_run_lane(move |path| {
        if let Some(ui) = weak.upgrade() {
            match run_lane(&path, &ui.get_command_text()) {
                Ok(lane) => {
                    refresh(&ui, false);
                    ui.set_message(format!("Started {} at {}", lane.branch, lane.url()).into());
                    ui.set_has_error(false);
                }
                Err(error) => {
                    ui.set_message(format!("Could not start worktree: {error}").into());
                    ui.set_has_error(true);
                }
            }
        }
    });

    let weak = ui.as_weak();
    ui.on_stop_lane(move |path| {
        if let Some(ui) = weak.upgrade() {
            match stop(Path::new(path.as_str())) {
                Ok(lane) => {
                    refresh(&ui, false);
                    ui.set_message(format!("Stopped {}", lane.branch).into());
                    ui.set_has_error(false);
                }
                Err(error) => {
                    ui.set_message(format!("Could not stop worktree: {error}").into());
                    ui.set_has_error(true);
                }
            }
        }
    });

    let weak = ui.as_weak();
    ui.on_open_url(move |url| {
        if let Some(ui) = weak.upgrade() {
            if let Err(error) = webbrowser::open(&url) {
                ui.set_message(format!("Could not open browser: {error}").into());
                ui.set_has_error(true);
            }
        }
    });

    let weak = ui.as_weak();
    ui.on_open_log(move |branch, port| {
        if let Some(ui) = weak.upgrade() {
            ui.set_log_title(format!("{} · :{}", branch, port).into());
            ui.set_log_port(port);
            ui.set_log_visible(true);
            refresh_log(&ui);
        }
    });

    let weak = ui.as_weak();
    ui.on_refresh_log(move || {
        if let Some(ui) = weak.upgrade() {
            refresh_log(&ui);
        }
    });

    refresh(&ui, true);
    let refresh_timer = Timer::default();
    let weak = ui.as_weak();
    refresh_timer.start(TimerMode::Repeated, Duration::from_secs(4), move || {
        if let Some(ui) = weak.upgrade() {
            refresh(&ui, false);
            if ui.get_log_visible() {
                refresh_log(&ui);
            }
        }
    });
    ui.run()?;
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::split_command;

    #[test]
    fn windows_command_paths_keep_backslashes_and_quoted_spaces() {
        let args = split_command(r#"C:\Tools\server.exe --flag "C:\My Data\file.txt""#).unwrap();
        assert_eq!(
            args,
            [r"C:\Tools\server.exe", "--flag", r"C:\My Data\file.txt",]
        );
    }
}
