# Four worktrees, four local URLs

This zero-dependency Node app shows which Lanes worktree is serving each URL. From the repository root, install `lanes` and create three worktrees:

```sh
cargo install --path crates/lanes-core
git worktree add ../lanes-auth -b demo/auth
git worktree add ../lanes-payments -b demo/payments
git worktree add ../lanes-dashboard -b demo/dashboard
```

In each worktree, open a terminal and run:

```sh
cd examples/branch-demo
lanes run node server.mjs
```

Then run `lanes status` from any worktree to see four different URLs. Open them in a browser to see the matching branch. The server also exposes `/health` as JSON.

Use `lanes stop` from a worktree to stop its server. The demo needs Node.js; it does not need npm packages or a WebView.
