# Native desktop app

The desktop app is a native Slint window backed by the same Rust core as the CLI. It uses a software renderer and a native window backend; it does not embed a browser or WebView. **Open** launches the URL in your system browser.

## Start the app

The release archive contains `lanes-desktop` next to `lanes` (`.exe` on Windows). You can also run it from a source checkout:

```sh
cargo run -p lanes-desktop
```

Git must be installed and available on `PATH`. You can pass a repository directory when launching the app:

```sh
lanes-desktop /path/to/project
```

Without an argument, the app uses its current directory if it is a Git repository, then the last successfully opened project. On first launch from a release folder, choose a repository with **Browse…** or paste its path into **Project directory** and click **Refresh**. The app remembers that directory for the next launch.

## Controls

| Control | Action |
| --- | --- |
| **Project directory** | Choose any folder inside a Git repository. Lanes discovers its worktrees through Git. |
| **Run command** | Command and arguments used by **Run** for any worktree row; defaults to `npm run dev`. |
| **Refresh** | Reload worktrees and tracked process state. The app also refreshes automatically every four seconds. |
| **Run** | Start the command in that worktree's root directory with its lane environment. |
| **Stop** | Stop the tracked command for that worktree. |
| **Logs** | Show recent output from that row's desktop-launched command. |
| **Open** | Open the row's assigned URL in the system browser when its tracked process is running. |

The run command is split into an executable and arguments. Quotes can group an argument that contains spaces. It is not a shell script field: for a pipeline or a sequence of commands, put the logic in a project script and run that script. On Windows, Lanes handles `.cmd` and `.bat` executable shims, including `npm.cmd`.

The desktop app starts its command at the **worktree root**. The CLI starts from your **current directory**. In a monorepo, use a root-level script for desktop runs or use `lanes run` from the package directory in a terminal.

## Logs and state

Desktop commands write stdout and stderr to `lane-PORT.log` in the [Lanes data directory](cli.md#environment-and-data). A new desktop run on that port starts a fresh log. The **Logs** panel shows the last 64 KiB and refreshes every four seconds while open. CLI runs keep output in their terminal instead.

Logs are named by port. If an old lane is removed and its port is later reused, **Logs** may show the previous lane's output until the new lane has been run from the desktop app.

The row's “running” state means that the recorded process ID and start time still match a live process. It does not mean that the application has bound its port or passed an HTTP health check. If **Open** reaches no server, inspect **Logs** and confirm the application reads `PORT`.

Closing the log panel does not stop the command. Use **Stop** on the row when you want Lanes to terminate it.

## Current scope

The desktop app manages one command and one assigned port per worktree. It does not yet manage a frontend/backend pair, edit `.env` files, configure hostnames or HTTPS, or create Git worktrees. Git remains responsible for creating and removing worktrees.
