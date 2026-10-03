#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_installer {
    slint::include_modules!();

    use slint::{Timer, TimerMode};
    use std::fs;
    use std::io;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::mpsc;
    use std::time::Duration;
    use winreg::enums::{RegType, HKEY_CURRENT_USER};
    use winreg::{RegKey, RegValue};

    const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Lanes";
    const MARKER: &str = "lanes-install-state.txt";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    enum Event {
        Progress(i32, String),
        Complete(PathBuf),
        Failed(String),
    }

    fn default_location() -> io::Result<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::other("LOCALAPPDATA is not set"))?;
        Ok(PathBuf::from(local).join("Programs").join("Lanes"))
    }

    fn validate_location(path: &Path) -> io::Result<()> {
        if !path.is_absolute() || path.parent().is_none_or(|parent| parent == path) {
            return Err(io::Error::other(
                "Choose a full install path inside a folder",
            ));
        }
        if path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(io::Error::other("Install path cannot contain '..'"));
        }
        Ok(())
    }

    fn payload(name: &str) -> io::Result<Vec<u8>> {
        #[cfg(feature = "bundled")]
        {
            return match name {
                "lanes.exe" => Ok(include_bytes!(concat!(env!("OUT_DIR"), "/lanes.exe")).to_vec()),
                "lanes-desktop.exe" => {
                    Ok(include_bytes!(concat!(env!("OUT_DIR"), "/lanes-desktop.exe")).to_vec())
                }
                _ => Err(io::Error::other("unknown payload")),
            };
        }
        #[cfg(not(feature = "bundled"))]
        {
            fs::read(std::env::current_exe()?.parent().unwrap().join(name))
        }
    }

    fn start_menu_shortcut() -> io::Result<PathBuf> {
        let roaming =
            std::env::var_os("APPDATA").ok_or_else(|| io::Error::other("APPDATA is not set"))?;
        Ok(PathBuf::from(roaming).join(r"Microsoft\Windows\Start Menu\Programs\Lanes.lnk"))
    }

    fn desktop_shortcut() -> io::Result<PathBuf> {
        let desktop =
            dirs::desktop_dir().ok_or_else(|| io::Error::other("Desktop folder not found"))?;
        Ok(desktop.join("Lanes.lnk"))
    }

    fn create_shortcut(link: &Path, target: &Path) -> io::Result<()> {
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent)?;
        }
        let script = "$s=New-Object -ComObject WScript.Shell; $l=$s.CreateShortcut($env:LANES_LINK); $l.TargetPath=$env:LANES_TARGET; $l.WorkingDirectory=$env:LANES_WORKDIR; $l.Description='Lanes - Run every branch'; $l.Save()";
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .env("LANES_LINK", link)
            .env("LANES_TARGET", target)
            .env("LANES_WORKDIR", target.parent().unwrap())
            .status()?;
        if !status.success() {
            return Err(io::Error::other("Windows could not create the shortcut"));
        }
        Ok(())
    }

    fn path_segments(value: &str) -> impl Iterator<Item = &str> {
        value
            .split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty())
    }

    fn same_segment(segment: &str, target: &Path) -> bool {
        segment
            .trim_matches('"')
            .eq_ignore_ascii_case(&target.to_string_lossy())
    }

    fn write_user_path(path: &Path, add: bool) -> io::Result<bool> {
        let environment = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
            "Environment",
            winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
        )?;
        let original: String = environment.get_value("Path").unwrap_or_default();
        let exists = path_segments(&original).any(|part| same_segment(part, path));
        if add && exists {
            return Ok(false);
        }
        if !add && !exists {
            return Ok(false);
        }
        let updated = if add {
            format!(
                "{}{}{}",
                original,
                if original.is_empty() || original.ends_with(';') {
                    ""
                } else {
                    ";"
                },
                path.display()
            )
        } else {
            path_segments(&original)
                .filter(|part| !same_segment(part, path))
                .collect::<Vec<_>>()
                .join(";")
        };
        let encoded: Vec<u8> = updated
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        environment.set_raw_value(
            "Path",
            &RegValue {
                bytes: encoded,
                vtype: RegType::REG_EXPAND_SZ,
            },
        )?;
        broadcast_environment();
        Ok(true)
    }

    fn broadcast_environment() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        };
        let name: Vec<u16> = "Environment".encode_utf16().chain(Some(0)).collect();
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                name.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                2000,
                std::ptr::null_mut(),
            );
        }
    }

    fn register_uninstaller(location: &Path) -> io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY)?;
        key.set_value("DisplayName", &"Lanes")?;
        key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
        key.set_value("Publisher", &"Lanes")?;
        key.set_value("InstallLocation", &location.to_string_lossy().as_ref())?;
        key.set_value(
            "DisplayIcon",
            &location
                .join("lanes-desktop.exe")
                .to_string_lossy()
                .as_ref(),
        )?;
        key.set_value(
            "UninstallString",
            &format!(
                "\"{}\" --uninstall",
                location.join("Lanes Uninstall.exe").display()
            ),
        )?;
        key.set_value("NoModify", &1u32)?;
        key.set_value("NoRepair", &1u32)?;
        Ok(())
    }

    fn install(
        location: PathBuf,
        add_path: bool,
        add_desktop: bool,
        sender: mpsc::Sender<Event>,
    ) -> io::Result<()> {
        validate_location(&location)?;
        let previous = fs::read_to_string(location.join(MARKER)).unwrap_or_default();
        let previous_path_added = previous.lines().any(|line| line == "path_added=true");
        let previous_desktop = previous.lines().any(|line| line == "desktop_shortcut=true");
        sender
            .send(Event::Progress(10, "Preparing files…".into()))
            .ok();
        let cli = payload("lanes.exe")?;
        let desktop = payload("lanes-desktop.exe")?;
        fs::create_dir_all(&location)?;
        sender
            .send(Event::Progress(
                35,
                "Installing the CLI and desktop app…".into(),
            ))
            .ok();
        fs::write(location.join("lanes.exe"), cli)?;
        fs::write(location.join("lanes-desktop.exe"), desktop)?;
        fs::copy(
            std::env::current_exe()?,
            location.join("Lanes Uninstall.exe"),
        )?;
        sender
            .send(Event::Progress(65, "Creating shortcuts…".into()))
            .ok();
        create_shortcut(&start_menu_shortcut()?, &location.join("lanes-desktop.exe"))?;
        if add_desktop {
            create_shortcut(&desktop_shortcut()?, &location.join("lanes-desktop.exe"))?;
        } else if previous_desktop {
            if let Ok(link) = desktop_shortcut() {
                let _ = fs::remove_file(link);
            }
        }
        sender
            .send(Event::Progress(85, "Finishing Windows setup…".into()))
            .ok();
        let path_added = if add_path {
            write_user_path(&location, true)? || previous_path_added
        } else {
            if previous_path_added {
                write_user_path(&location, false)?;
            }
            false
        };
        register_uninstaller(&location)?;
        fs::write(
            location.join(MARKER),
            format!("path_added={path_added}\ndesktop_shortcut={add_desktop}\n"),
        )?;
        sender
            .send(Event::Progress(100, "Installation complete".into()))
            .ok();
        Ok(())
    }

    fn remove_installation(location: &Path) -> io::Result<()> {
        validate_location(location)?;
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(UNINSTALL_KEY)?;
        let registered: String = key.get_value("InstallLocation")?;
        if !Path::new(&registered)
            .to_string_lossy()
            .eq_ignore_ascii_case(&location.to_string_lossy())
        {
            return Err(io::Error::other(
                "Install location does not match Windows registration",
            ));
        }
        let state = fs::read_to_string(location.join(MARKER))?;
        if state.lines().any(|line| line == "path_added=true") {
            write_user_path(location, false)?;
        }
        if state.lines().any(|line| line == "desktop_shortcut=true") {
            if let Ok(link) = desktop_shortcut() {
                let _ = fs::remove_file(link);
            }
        }
        if let Ok(link) = start_menu_shortcut() {
            let _ = fs::remove_file(link);
        }
        for name in [
            "lanes.exe",
            "lanes-desktop.exe",
            "Lanes Uninstall.exe",
            MARKER,
        ] {
            let path = location.join(name);
            for attempt in 0..20 {
                match fs::remove_file(&path) {
                    Ok(()) => break,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                    Err(error) if attempt == 19 => return Err(error),
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        }
        let _ = fs::remove_dir(location);
        RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY)?;
        Ok(())
    }

    fn launch_remove_helper() -> io::Result<()> {
        let current = std::env::current_exe()?;
        let location = current
            .parent()
            .ok_or_else(|| io::Error::other("cannot locate installation"))?;
        fs::read_to_string(location.join(MARKER))?;
        let temporary =
            std::env::temp_dir().join(format!("lanes-uninstall-{}.exe", std::process::id()));
        fs::copy(&current, &temporary)?;
        Command::new(temporary)
            .arg("--remove")
            .arg(location)
            .spawn()?;
        Ok(())
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let arguments: Vec<_> = std::env::args_os().collect();
        if arguments.get(1).is_some_and(|arg| arg == "--remove") {
            let location = arguments.get(2).ok_or("missing install location")?;
            remove_installation(Path::new(location))?;
            return Ok(());
        }
        let ui = InstallerWindow::new()?;
        ui.set_location(default_location()?.to_string_lossy().into_owned().into());
        let uninstaller_name = std::env::current_exe()?
            .file_stem()
            .is_some_and(|name| name.to_string_lossy().contains("Uninstall"));
        if arguments.get(1).is_some_and(|arg| arg == "--uninstall") || uninstaller_name {
            ui.set_page(4);
        }
        let (sender, receiver) = mpsc::channel();
        let weak = ui.as_weak();
        ui.on_install(move |location, add_path, add_desktop| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let location = PathBuf::from(location.as_str());
            if let Err(error) = validate_location(&location) {
                ui.set_status(error.to_string().into());
                ui.set_has_error(true);
                ui.set_page(2);
                return;
            }
            ui.set_page(2);
            ui.set_progress(0);
            ui.set_status("Starting installation…".into());
            ui.set_has_error(false);
            let sender = sender.clone();
            std::thread::spawn(move || {
                match install(location.clone(), add_path, add_desktop, sender.clone()) {
                    Ok(()) => {
                        let _ = sender.send(Event::Complete(location));
                    }
                    Err(error) => {
                        let _ = sender.send(Event::Failed(error.to_string()));
                    }
                }
            });
        });
        let weak = ui.as_weak();
        ui.on_launch_app(move || {
            if let Some(ui) = weak.upgrade() {
                let _ =
                    Command::new(Path::new(ui.get_location().as_str()).join("lanes-desktop.exe"))
                        .spawn();
            }
        });
        let weak = ui.as_weak();
        ui.on_close_installer(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().hide().ok();
            }
        });
        let weak = ui.as_weak();
        ui.on_uninstall(move || {
            if let Some(ui) = weak.upgrade() {
                match launch_remove_helper() {
                    Ok(()) => {
                        ui.window().hide().ok();
                    }
                    Err(error) => {
                        ui.set_status(error.to_string().into());
                        ui.set_has_error(true);
                    }
                }
            }
        });
        let weak = ui.as_weak();
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            while let Ok(event) = receiver.try_recv() {
                match event {
                    Event::Progress(value, message) => {
                        ui.set_progress(value);
                        ui.set_status(message.into());
                    }
                    Event::Complete(location) => {
                        ui.set_location(location.to_string_lossy().into_owned().into());
                        ui.set_page(3);
                    }
                    Event::Failed(error) => {
                        ui.set_status(error.into());
                        ui.set_has_error(true);
                    }
                }
            }
        });
        ui.run()?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn path_comparison_is_case_insensitive_and_exact() {
            let target = Path::new(r"C:\Users\Example\Lanes");
            assert!(same_segment(r"c:\users\example\lanes", target));
            assert!(!same_segment(r"C:\Users\Example\Lanes-old", target));
        }

        #[test]
        fn install_location_rejects_relative_and_parent_paths() {
            assert!(validate_location(Path::new("Lanes")).is_err());
            assert!(validate_location(Path::new(r"C:\Users\Example\..\Lanes")).is_err());
            assert!(validate_location(Path::new(r"C:\Users\Example\Lanes")).is_ok());
        }

        #[test]
        fn shortcut_points_to_executable() {
            let directory = tempfile::tempdir().unwrap();
            let link = directory.path().join("Lanes.lnk");
            create_shortcut(&link, &std::env::current_exe().unwrap()).unwrap();
            assert!(link.metadata().unwrap().len() > 0);
        }
    }
}

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_installer::run() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let message: Vec<u16> = format!("Lanes setup could not finish:\n{error}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let title: Vec<u16> = "Lanes setup".encode_utf16().chain(Some(0)).collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!(
        "The Lanes installer is available on Windows. Use the release archive on this system."
    );
}
