# Development

Lanes is a Cargo workspace. `lanes-core` provides the shared Rust library and `lanes` executable. `lanes-desktop` builds a native Slint window against that library. The [architecture notes](architecture.md) describe how both clients share the registry and process lifecycle.

## Prerequisites

- Git
- A current stable [Rust toolchain](https://rustup.rs/) with Cargo and rustfmt
- On Windows, the Visual Studio C++ build tools for the MSVC target
- On Linux, development libraries for the desktop UI

For an Ubuntu or Debian desktop build, the CI job installs:

```sh
sudo apt-get update
sudo apt-get install -y libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libfontconfig1-dev libfreetype6-dev
```

The CLI can be built independently if you do not need the desktop UI.

## Build and run

From the repository root:

```sh
cargo build -p lanes-core
cargo build -p lanes-desktop
cargo run -p lanes-core --bin lanes -- help
cargo run -p lanes-desktop
```

To try the CLI with the included Node server:

```sh
cd examples/branch-demo
cargo run -p lanes-core --bin lanes -- run node server.mjs
```

The demo needs Node.js but no installed npm packages. Its [README](../examples/branch-demo/README.md) explains the four-worktree version.

## Checks

```sh
cargo fmt --all --check
cargo test -p lanes-core
cargo check -p lanes-desktop
```

GitHub Actions runs these checks on Windows, Ubuntu, and macOS. Desktop-specific unit tests can be run with `cargo test -p lanes-desktop` on a machine with the relevant UI build dependencies.

## Repository map

```text
crates/lanes-core/src/lib.rs       Git, registry, ports, process lifecycle
crates/lanes-core/src/main.rs      CLI parsing and output
crates/lanes-desktop/src/main.rs   Native app callbacks and process/log wiring
crates/lanes-desktop/ui/app.slint  Window layout and controls
examples/branch-demo/             Minimal PORT-aware HTTP server
docs/                             Guides and diagrams
```

The `CI` workflow checks each push and pull request. A tag matching `v*` starts the `Release` workflow, which builds the CLI and desktop binaries and attaches Windows x64, Linux x64, and macOS Apple Silicon archives to a GitHub release.

## Before a pull request

Keep CLI behavior and desktop behavior aligned through the shared library. Add a focused test when changing port allocation, worktree discovery, or process lifecycle. Run the checks above and document any behavior that changes for users.
