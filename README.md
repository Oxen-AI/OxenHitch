# Oxen Hitch

Run Claude Desktop on the [Oxen.ai](https://oxen.ai) gateway. Hitch lives in your menu bar: paste
your Oxen API key, click Connect, restart Claude. No developer mode, no config files.

- **Connect** — validates your key, then sets Claude Desktop up for you.
- **Switch back** — one toggle returns Claude to Anthropic sign-in, and back again.
- **Models** — choose which Oxen models Claude shows and which is the default.
- **Status** — see at a glance whether Claude is on Oxen and whether the gateway is reachable.
- **Reversible** — Remove restores whatever Claude had before.

macOS first. Windows and Linux are planned; the core already knows where Claude keeps its config on each.

## Develop

Requires Rust, Node 22+ and pnpm.

```sh
pnpm install
pnpm tauri dev      # run the app
pnpm test           # UI tests
pnpm test:rust      # core tests
pnpm tauri build    # build Hitch.app and a .dmg
```

To try it without changing your real Claude setup:

```sh
HITCH_CLAUDE_DIR=/tmp/hitch-sandbox/Claude-3p pnpm tauri dev
```

## Learn more

- [`docs/01_project_brief.md`](docs/01_project_brief.md) — what Hitch is and how Claude Desktop's third-party mode works
- [`docs/02_tech_stack.md`](docs/02_tech_stack.md) — the stack, layout and commands
- [`docs/03_decisions.md`](docs/03_decisions.md) — decisions and why
