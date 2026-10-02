# Four worktrees, four local URLs

This example is a small Node.js HTTP server. It prints the lane and port in the browser and exposes the same values as JSON at `/health`. It needs Node.js; npm is optional, and there are no packages to install.

## Create the worktrees

Install the CLI from the repository root (or use a release binary already on your `PATH`):

```sh
cargo install --path crates/lanes-core
git worktree add -b demo/auth ../lanes-demo-auth HEAD
git worktree add -b demo/payments ../lanes-demo-payments HEAD
git worktree add -b demo/dashboard ../lanes-demo-dashboard HEAD
```

These commands work in PowerShell and POSIX shells. Choose other branch or directory names if these already exist. Each new worktree starts from the current commit, so commit any changes you want the demo to include before creating them.

## Run all four branches

Open four terminals, each starting at the original repository root. Change into one directory per terminal:

| Terminal | Directory relative to the original repository root |
| --- | --- |
| 1 | `examples/branch-demo` |
| 2 | `../lanes-demo-auth/examples/branch-demo` |
| 3 | `../lanes-demo-payments/examples/branch-demo` |
| 4 | `../lanes-demo-dashboard/examples/branch-demo` |

In each terminal, run:

```sh
lanes run npm run dev
```

The CLI runs the command in the directory you invoked it from. You can use `lanes run node server.mjs` instead if you do not have npm. Each terminal remains attached to its server; leave it open while viewing the demo.

In a fifth terminal, from any directory inside one of these worktrees:

```sh
lanes status
```

Open the four URLs from the output. Each page shows a different lane and port. Add `/health` to a URL to check its JSON response. Ports come from the local Lanes registry, so they may differ from screenshots or other machines. Stop and rerun a lane to see that its assigned port stays stable.

The native desktop app starts commands at the **worktree root**. To run this example from the app, select this repository and set the command to `node examples/branch-demo/server.mjs` before clicking Run on a row.

## Stop and clean up

From a second terminal in each worktree, run `lanes stop` to stop that worktree's server. Once all four servers have stopped, run these commands from the original repository root:

```sh
git worktree remove ../lanes-demo-auth
git worktree remove ../lanes-demo-payments
git worktree remove ../lanes-demo-dashboard
git branch -d demo/auth demo/payments demo/dashboard
lanes prune
```

`lanes prune` releases the port reservations for worktrees that no longer exist. If you made commits on a demo branch, keep that branch instead of deleting it.
