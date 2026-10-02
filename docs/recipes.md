# Lanes recipes

Lanes gives a worktree one stable port and starts a command with `PORT`, `LANE_PORT`, `LANE`, and `BASE_URL` in its environment. Run commands from the project directory you normally use; the CLI preserves that working directory. The desktop app starts its command at the worktree root.

## Check a project before starting it

```sh
lanes doctor node
lanes env
lanes status
```

Run these inside a Git worktree. `doctor` checks Git, the worktree, registry, lane port, and the optional executable without starting it. `env` shows the values the child would receive. `status` discovers other worktrees in the same repository. Ports may differ between computers and can change if another process has taken a stopped lane's port.

## Node HTTP server

Read `PORT` when starting the server, and bind the service to that port:

```js
import { createServer } from "node:http";

const port = Number(process.env.PORT ?? 3000);
createServer((_request, response) => {
  response.end(`lane=${process.env.LANE ?? "none"}\nurl=${process.env.BASE_URL ?? ""}\n`);
}).listen(port, "127.0.0.1");
```

Then run `lanes run node server.mjs` from the directory containing the file. For a complete four-worktree example, see [branch-demo](../examples/branch-demo/README.md).

## Vite

Vite's default port is independent of `PORT`, and it can silently try another port when its chosen port is occupied. Read Lanes' `PORT` in your `vite.config.js` and enable `strictPort` so the URL shown by Lanes matches the server or startup fails clearly. [Vite server options](https://vite.dev/config/server-options)

```js
import { defineConfig } from "vite";

export default defineConfig({
  server: {
    host: "127.0.0.1",
    port: Number(process.env.PORT ?? 5173),
    strictPort: true,
  },
});
```

With a normal `"dev": "vite"` package script, run `lanes run npm run dev` from the package directory. The same config works in PowerShell, Command Prompt, Bash, and zsh because the port is passed to the child process environment rather than set with shell-specific syntax.

## Next.js

Next.js `next dev` reads an inherited `PORT` value. Keep a normal `"dev": "next dev"` package script and run `lanes run npm run dev` from the package directory. Set the port through Lanes rather than a project `.env` file: Next.js reads it while starting the HTTP server, before loading `.env`. [Next.js CLI documentation](https://nextjs.org/docs/app/api-reference/cli/next#changing-the-default-port)

## URLs and callbacks

`BASE_URL` is `http://localhost:<PORT>`. A server can derive local URLs from it:

```js
const callbackUrl = new URL("/auth/callback", process.env.BASE_URL).toString();
```

This only sets the child process environment. If an OAuth provider, webhook service, or browser client needs a URL configured separately, update that configuration for each lane as needed. Lanes does not rewrite project `.env` files or register external callback URLs.

## Use the lane environment with another command

`lanes env` prints dotenv-style assignments. For shell assignments, use one of the supported formats:

```sh
lanes env --shell sh
lanes env --shell powershell
```

The first is for POSIX shells; the second is for PowerShell on any operating system. To apply them in the current shell:

```sh
# Bash or zsh
eval "$(lanes env --shell sh)"
```

```powershell
# PowerShell
lanes env --shell powershell | Out-String | Invoke-Expression
```

Commands you start manually after this receive the variables, but Lanes cannot track or stop those processes. Use `lanes run` when you want `lanes status` and `lanes stop` to manage the process.

## Recover reservations after deleting worktrees

```sh
lanes prune
```

This removes saved lanes whose worktree directory no longer exists and whose recorded process is stopped. It keeps the ports assigned to existing worktrees. Run it after removing temporary worktrees if the `4300–4999` range fills up.
