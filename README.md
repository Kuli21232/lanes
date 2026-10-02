# Lanes

**Run every branch. No port collisions.**

Four Git worktrees. Four branches. One default port. What could possibly go wrong?

```text
Before Lanes              After Lanes
main      → :3000          main      → :4300
auth      → :3000 ✗        auth      → :4301
payments  → :3000 ✗        payments  → :4302
```

Lanes gives each worktree a stable local port and passes it to the command you run.

```text
$ lanes run npm run dev

LANES  shop · feature/auth
PORT=4301  http://localhost:4301
```

Open another worktree and run the same command. It gets another port. `lanes status` lists every worktree in the repository:

```text
LANES

main                     http://localhost:4300   ● running
feature/auth             http://localhost:4301   ● running
feature/payments         http://localhost:4302   ○ stopped

3 worktrees · 2 running
```

## Install

Download the CLI and native desktop app from [Releases](https://github.com/Kuli21232/lanes/releases). The Windows archive contains `lanes.exe` and `lanes-desktop.exe`; the Linux and macOS archives contain `lanes` and `lanes-desktop`. The macOS archive targets Apple Silicon.

To build from source, install Git and a [Rust toolchain](https://rustup.rs/). On Windows, Rust also needs the Visual Studio C++ build tools.

```sh
cargo install --git https://github.com/Kuli21232/lanes --package lanes-core
```

The executable is called `lanes`. In a development checkout:

```sh
cargo run -p lanes-core --bin lanes -- run npm run dev
```

## Commands

| Command | What it does |
| --- | --- |
| `lanes run [command...]` | Run a command in the current worktree. Defaults to `npm run dev`. |
| `lanes status` | Discover and display the repository's worktrees and their local URLs. |
| `lanes stop` | Stop the process Lanes started in the current worktree. |
| `lanes` | Shortcut for `lanes status`. |

Lanes sets `PORT`, `LANE_PORT`, `LANE` and `BASE_URL` in the child environment. Applications must read `PORT` to bind to the assigned port. If your tool ignores `PORT`, use a script that reads it. For Vite, a small cross-platform Node launcher works:

```js
import { spawn } from "node:child_process";
const child = spawn("vite", ["--port", process.env.PORT], { stdio: "inherit", shell: true });
child.on("exit", code => process.exit(code ?? 1));
```

Then point `npm run dev` at that script. Other tools, including many Node servers, already honor `PORT` directly.

## Native desktop app

The dashboard is a native [Slint](https://slint.dev/) window backed by the same Rust core as the CLI. It has no browser engine, HTML layer or WebView. Enter a path inside a Git repository, refresh the list, and run or stop a worktree with the configured command.

```sh
cargo run -p lanes-desktop
```

The desktop app writes background command output to `lane-PORT.log` in the Lanes data directory. The registry is stored in the same directory: `%LOCALAPPDATA%\Lanes` on Windows and the user data directory on Linux/macOS. Set `LANES_HOME` to override it.

## Try four branches

The [branch demo](examples/branch-demo/README.md) is a zero-dependency Node server that shows the current lane and port in a browser. Run it from four worktrees to see four stable URLs.

## How ports stay stable

Lanes identifies a worktree by its canonical path. Its shared registry reserves one port in `4300–4999` per worktree. An interprocess file lock protects allocation. On launch, Lanes checks that the assigned port is available; if another application occupies it, Lanes assigns a new free port. Stopped worktrees retain their assignment. Deleting a worktree does not currently prune its reservation automatically.

The initial release manages one service port per worktree. It does not yet rewrite `.env` files, configure OAuth callbacks, proxy multiple services, or guarantee that a child application listens on `PORT`. Those are separate future capabilities.

## Development

```sh
cargo fmt --all --check
cargo test -p lanes-core
cargo check -p lanes-desktop
```

The source is MIT licensed. Issues and pull requests are welcome.
