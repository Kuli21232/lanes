# Getting started

Lanes needs Git and an application that can listen on the port in `PORT`. It does not create worktrees for you; use Git's normal `worktree` commands.

## 1. Install Lanes

Download the current archive from [GitHub Releases](https://github.com/Kuli21232/lanes/releases/latest).

| System | Archive | Executables |
| --- | --- | --- |
| Windows x64 | `lanes-windows-x64.zip` | `lanes.exe`, `lanes-desktop.exe` |
| Linux x64 | `lanes-linux-x64.tar.gz` | `lanes`, `lanes-desktop` |
| macOS Apple Silicon | `lanes-macos-arm64.tar.gz` | `lanes`, `lanes-desktop` |

Extract the archive, then run the executable in that directory (for example `./lanes --help` on Linux/macOS or `.\lanes.exe --help` in PowerShell). Add the directory to your `PATH` if you want to call `lanes` from anywhere.

For example, in PowerShell on Windows:

```powershell
Invoke-WebRequest https://github.com/Kuli21232/lanes/releases/latest/download/lanes-windows-x64.zip -OutFile lanes.zip
Expand-Archive -LiteralPath .\lanes.zip -DestinationPath .\lanes-bin
.\lanes-bin\lanes.exe --help
```

On Linux x64, extract the matching archive and run the binary in place:

```sh
curl -fL https://github.com/Kuli21232/lanes/releases/latest/download/lanes-linux-x64.tar.gz -o lanes.tar.gz
mkdir -p lanes-bin
tar -xzf lanes.tar.gz -C lanes-bin
./lanes-bin/lanes --help
```

Use `lanes-macos-arm64.tar.gz` in those commands on an Apple Silicon Mac. These examples keep the binaries in the current directory; moving them to a directory on `PATH` is optional.

To install the CLI from source instead:

```sh
cargo install --git https://github.com/Kuli21232/lanes --package lanes-core --bin lanes
```

This installs the CLI only. The desktop app can be built from a checkout with `cargo build --release -p lanes-desktop`.

## 2. Run your first worktree

Open a terminal in an existing Git repository:

```sh
cd my-app
lanes doctor npm
lanes run npm run dev
```

`lanes doctor npm` checks whether Git and `npm` are available and whether Lanes can prepare a port. `lanes run` stays attached to the command and prints its URL. Leave this terminal open.

The application must use `PORT`. If it always binds its own default port, update its dev script first; see the [recipes](recipes.md). Lanes does not rewrite an application's configuration.

## 3. Open another branch

From a second terminal in the first worktree, create a branch and worktree:

```sh
git worktree add -b feature/auth ../my-app-auth
cd ../my-app-auth
lanes run npm run dev
```

The second command receives a different port. In a third terminal, run `lanes status` from either worktree to list their URLs:

```text
LANES

main                     http://localhost:4300   ● running
feature/auth             http://localhost:4301   ● running

2 worktrees · 2 running
```

The exact port numbers depend on earlier Lanes reservations and other programs on your computer. `status` reports the tracked process state; open the URL to verify that the application is ready to serve requests.

To stop one branch from a separate terminal inside its worktree:

```sh
lanes stop
```

The reservation remains, so the next run normally receives the same port.

## Try a server that already reads PORT

The [branch demo](../examples/branch-demo/README.md) is a small Node.js HTTP server with no npm dependencies. It displays `LANE`, `PORT`, and `BASE_URL` in the browser and provides a `/health` endpoint. It is useful for checking the workflow before adapting a larger project.

## Use the desktop app

Start `lanes-desktop` from the release archive. Select any folder inside your Git repository with **Browse…**, paste its path and click **Refresh**, or launch `lanes-desktop /path/to/project`. Lanes lists the repository's worktrees and remembers the selected project for the next launch. Enter a run command such as `npm run dev`, then click **Run** on a row. Use **Logs** to inspect the command's output and **Open** to visit its URL.

The desktop command runs from the selected worktree's root. If your server lives in a subdirectory, use a project script that changes to it, or run the CLI from that subdirectory. See the [desktop guide](desktop.md).

## Next

- [CLI reference](cli.md): commands, environment, and exit behavior
- [Desktop guide](desktop.md): controls and logs
- [Architecture](architecture.md): registry and process lifecycle
- [Troubleshooting](troubleshooting.md): common setup failures
