# CLI reference

Run Lanes from any directory inside a Git worktree. The CLI detects the worktree root through Git, but runs your command from the directory where you invoked `lanes`. This matters in monorepos: `cd apps/web && lanes run npm run dev` runs the script in `apps/web`.

```text
lanes run [command...]
lanes status
lanes env [--shell dotenv|sh|powershell]
lanes doctor [command...]
lanes stop
lanes prune
lanes help
```

`lanes` with no arguments is equivalent to `lanes status`.

## `run`

```sh
lanes run npm run dev
lanes run -- node server.mjs
lanes run
```

With no command, `run` starts `npm run dev`. The optional `--` separates Lanes arguments from the child command. Lanes starts one process, forwards its input and output to your terminal, waits for it to exit, and returns its exit code when available.

`run` passes the lane variables below to the child. It does not run an HTTP readiness check. A printed URL means that a port was assigned and the process started; the server still needs to listen on that port.

If Lanes already tracks a live command in this worktree, a second `run` fails instead of starting another copy. To stop it from another terminal, use `lanes stop` in the same worktree.

## `status`

```sh
lanes status
```

Inside a Git repository, `status` discovers all its worktrees and reserves a port for each newly discovered one. Outside a Git repository, it lists all saved lanes on this computer. “Running” means Lanes still sees the tracked process, not that the URL responds to HTTP requests.

## `env`

```sh
lanes env
lanes env --shell sh
lanes env --shell powershell
```

`env` prepares a lane and prints its values without starting a process. The default `dotenv` format prints quoted assignments. `sh` prints `export` statements for POSIX shells (aliases `bash` and `zsh` also work). `powershell` prints PowerShell assignments (alias `pwsh`).

| Variable | Example | Meaning |
| --- | --- | --- |
| `PORT` | `4301` | Port for the child application's listener |
| `LANE_PORT` | `4301` | Same port under a Lanes-specific name |
| `LANE` | `feature-auth` | Branch name with `/` and `\` changed to `-` |
| `BASE_URL` | `http://localhost:4301` | Local URL formed from the assigned port |

The values are generated from the current worktree. `env` may assign its first port or replace an idle reservation that another process has occupied. It does not edit or load a project's `.env` files.

## `doctor`

```sh
lanes doctor
lanes doctor npm
lanes doctor -- node server.mjs
```

`doctor` checks that Git runs, the current directory belongs to a worktree, the registry can be opened, and the selected lane port is available or occupied by the tracked lane process. If a command is supplied, it checks whether the first word names an executable. A bare name is searched on `PATH`; Windows also checks the current directory. A relative path such as `./tool` is resolved from the current directory. `doctor` does not run the executable or inspect the application's network behavior. Failed checks return exit code 1.

## `stop`

```sh
lanes stop
```

`stop` targets the process recorded for the current worktree. On Windows it uses `taskkill` for the process tree. On Unix, it signals the tracked process tree or process group and waits for the lane port to become free. The lane remains in the registry with its reserved port.

If a child has already exited, `stop` clears its tracked process state. A process started outside Lanes is not managed by `stop`.

## `prune`

```sh
lanes prune
```

`prune` scans the saved registry across repositories and releases lanes whose worktree directory no longer exists and whose tracked process is not running. Existing worktree directories keep their reservations. Use this after removing old worktrees with Git.

## Environment and data

Lanes stores `registry.json` and `registry.lock` in the operating system's local user data directory under `Lanes`. On Windows this is normally `%LOCALAPPDATA%\Lanes`. Set `LANES_HOME` to use a different directory, for example to isolate a test setup:

```sh
LANES_HOME=/tmp/lanes-test lanes status
```

In PowerShell:

```powershell
$env:LANES_HOME = 'C:\temp\lanes-test'
lanes status
```

`LANES_HOME` must be set in every CLI or desktop process that should share that alternate registry. Desktop runs also write `lane-PORT.log` there. CLI runs write to the terminal.

The registry is local to your computer. It is not part of the Git repository and is not shared with teammates.
