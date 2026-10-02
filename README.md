# Lanes

**Run every branch. No port collisions.**

![Four Git worktrees, each with its own local port](docs/assets/lanes-overview.svg)

Git worktrees make it easy to keep several branches open. Local development servers still tend to ask for the same port. Lanes assigns a port to each worktree, remembers it between runs, and passes it to your command.

```text
Terminal 1 · main                Terminal 2 · feature/auth
$ lanes run npm run dev          $ lanes run npm run dev
PORT=4300                       PORT=4301
http://localhost:4300           http://localhost:4301
```

Ports above are examples. The actual port depends on the reservations already on your machine.

Lanes includes a Rust CLI and a native desktop app built with [Slint](https://slint.dev/). The desktop app does not use a WebView.

## Start here

1. Download the archive for your machine from [Releases](https://github.com/Kuli21232/lanes/releases/latest): Windows x64, Linux x64, or macOS Apple Silicon. Extract it and place `lanes` on your `PATH`, or run it from the extracted directory. Each archive also contains `lanes-desktop`.
2. In a Git repository, start your project in one terminal:

   ```sh
   lanes run npm run dev
   ```

3. Open a second terminal in another worktree of the same repository and run the same command. From either worktree, use `lanes status` to see both URLs.

Your server must read `PORT` or be configured to use it. Lanes cannot change a port hardcoded inside the server. For a working example that needs no npm packages, try the [four-worktree demo](examples/branch-demo/README.md).

**New to worktrees?** The [getting started guide](docs/getting-started.md) covers installation, creating a second worktree, and checking that both servers respond.

## Commands

| Command | Purpose |
| --- | --- |
| `lanes run [command...]` | Run a command in the current worktree with its lane environment. With no command, runs `npm run dev`. |
| `lanes status` | List worktrees, their reserved URLs, and tracked process state. Outside a Git repository, lists saved lanes. |
| `lanes env [--shell FORMAT]` | Print `PORT`, `LANE_PORT`, `LANE`, and `BASE_URL` as `dotenv`, `sh`, or `powershell`. |
| `lanes doctor [command...]` | Check Git, worktree detection, registry access, the lane port, and optionally an executable. |
| `lanes stop` | Stop the command Lanes tracks in the current worktree. |
| `lanes prune` | Remove reservations for deleted worktrees without a running tracked process. |
| `lanes` | Shortcut for `lanes status`. |

Lanes passes these variables to the child process:

```text
PORT=4301
LANE_PORT=4301
LANE=feature-auth
BASE_URL=http://localhost:4301
```

It does not edit `.env` files. See the [CLI reference](docs/cli.md) for exact command behavior and the [recipes](docs/recipes.md) for project integration examples.

## Desktop app

Launch `lanes-desktop` from the extracted archive, or build and run it from source:

```sh
cargo run -p lanes-desktop
```

Choose a folder inside your repository. The app lists its worktrees and their URLs. Set a run command, then use **Run**, **Stop**, **Logs**, and **Open** on each row. It refreshes process state every four seconds. Commands launched from the desktop app run from the worktree root; CLI commands run from the directory where you invoke `lanes`.

See the [desktop guide](docs/desktop.md) for controls, command parsing, and log locations.

## How it works

Lanes asks Git for the current worktree and the repository's worktree list. A registry in your user data directory maps each canonical worktree path to a port in `4300–4999`. An interprocess lock protects registry updates. A stopped worktree keeps its port unless another application has taken it when Lanes next prepares that lane.

![Git worktree discovery, registry, and native clients](docs/assets/architecture.svg)

Read the [architecture notes](docs/architecture.md) for port allocation, process tracking, storage, and current limits.

## Build and contribute

Install [Rust](https://rustup.rs/) and Git. Windows builds also need the Visual Studio C++ build tools. On Linux, the desktop build needs system libraries for X11, Wayland, fonts, and keyboard input; see [development setup](docs/development.md).

```sh
cargo fmt --all --check
cargo test -p lanes-core
cargo check -p lanes-desktop
```

Issues and pull requests are welcome. The project is [MIT licensed](LICENSE).
