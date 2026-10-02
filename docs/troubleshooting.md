# Troubleshooting

Start with `lanes doctor` from the worktree that fails. Add your executable name, for example `lanes doctor npm`, to check whether Lanes can find it. The check prepares the lane but does not start the server.

## “Not a Git repository” or no worktrees appear

Run `git status` and `git worktree list` from the same directory. In the desktop app, set **Project directory** to a folder inside the repository and click **Refresh**. Git must be installed and available on `PATH` to both the CLI and desktop app.

## The app still tries port 3000

Lanes sets `PORT` in the child environment. The application must read it or receive it through its own port option. Run `lanes env` to see the assigned value, then update your project dev script. The [recipes](recipes.md) show integration patterns. Lanes cannot override a server that hardcodes a different port.

## A URL is printed, but the browser cannot connect

The URL is assigned before the server has finished starting. Read the terminal output for a CLI run or open **Logs** for a desktop run. Check for missing dependencies, startup errors, and whether the application listens on `PORT`. `lanes status` reports the tracked process; it is not an HTTP health check.

If your app binds only to a different interface or uses HTTPS, its actual address may differ from `BASE_URL`. Lanes currently forms `http://localhost:PORT` and checks port availability on `127.0.0.1`.

## A port changed between runs

A stopped worktree normally keeps its reserved port. If another program occupies it when `run`, `env`, or `doctor` prepares that lane, Lanes selects a free replacement. `lanes status` can show the old reservation until the lane is prepared again. Stop the other program if you need the previous port and verify the current assignment with `lanes env`.

## “Already running”

Lanes tracks one command per worktree. Use `lanes status` to confirm the recorded process, then run `lanes stop` from that worktree or press **Stop** in the desktop app. A command started without Lanes is outside its process tracking.

## The command is missing in a second worktree

Git worktrees share committed history, not ignored build outputs. Files such as `node_modules` often need to be installed in each worktree. Run your project's normal dependency setup in that worktree, then retry. `lanes doctor npm` can check the executable, but it cannot check that your application's dependencies are installed.

## No ports left

Lanes allocates from `4300–4999` across all repositories in the local registry. Run `lanes prune` after deleting old worktrees. It removes only entries whose directory is gone and whose tracked process is stopped. It does not delete worktree files.

## Desktop app fails to open on Linux

The desktop app needs a graphical session and the platform libraries used by Slint's native window backend. For a source build on Ubuntu or Debian, install the packages listed in [development setup](development.md). If the folder picker is unavailable in your desktop environment, enter the repository path in **Project directory** and click **Refresh**.

## Logs are empty or outdated

Only desktop launches write `lane-PORT.log`. CLI output stays in its terminal. A new desktop run recreates the file for that port; the **Logs** panel displays only its last 64 KiB. If you assigned a new port, use **Logs** on the current row.

If none of these checks explains the problem, open an [issue](https://github.com/Kuli21232/lanes/issues) with your operating system, `lanes doctor` output, the command you ran, and the relevant server error. Remove secrets from logs before posting them.
