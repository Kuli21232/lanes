# Windows installer

Download `lanes-setup-windows-x64.exe` from the latest [GitHub release](https://github.com/Kuli21232/lanes/releases/latest). The installer is a native Rust and Slint app; it contains both `lanes.exe` and `lanes-desktop.exe`, so installation does not download anything else.

1. Open the installer and choose **Get started**.
2. Review the install location. The default is `%LOCALAPPDATA%\Programs\Lanes`, which does not require administrator access.
3. Leave **Add lanes to my PATH** selected to call the CLI from a new terminal. A desktop shortcut is optional. Click **Install Lanes**.
4. Choose **Finish** to open the desktop app. Its first-run setup asks for a Git repository path and your usual dev command.

The installer creates a Start menu shortcut and an entry in **Windows Settings → Apps → Installed apps**. Remove Lanes there or run `Lanes Uninstall.exe` from the installation directory with `--uninstall`. Uninstalling removes the installed binaries and shortcuts; it keeps the registry of worktree ports, desktop preferences and logs in `%LOCALAPPDATA%\Lanes` so a later installation can reuse them.

The portable `lanes-windows-x64.zip` remains available. It does not add a shortcut or update `PATH`; extract both executables and run them directly.

## Build an offline installer

From a Windows checkout with Rust and the MSVC toolchain:

```powershell
cargo build --release -p lanes-core -p lanes-desktop
$env:LANES_PAYLOAD_DIR = (Resolve-Path target/release).Path
cargo build --release -p lanes-installer --features bundled
```

The result is `target/release/lanes-installer.exe`. The release workflow renames it to `lanes-setup-windows-x64.exe`.
