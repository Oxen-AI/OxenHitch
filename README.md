# Oxen Hitch

Run Claude Desktop on the [Oxen.ai](https://oxen.ai) gateway. Hitch lives in your menu bar: paste
your Oxen API key, click Connect, restart Claude. No developer mode, no config files.

- **Connect** — validates your key, then sets Claude Desktop up for you.
- **Switch back** — one toggle returns Claude to Anthropic sign-in, and back again.
- **Models** — choose which Oxen models Claude shows and which is the default.
- **Status** — see at a glance whether Claude is on Oxen and whether the gateway is reachable.
- **Reversible** — Remove restores whatever Claude had before.

macOS first. Windows and Linux are planned; the core already knows where Claude keeps its config on each.

## Install

Download the latest `.dmg` from [Releases](https://github.com/Oxen-AI/OxenHitch/releases), open it,
and drag **Hitch** into **Applications**. It runs on Apple Silicon and Intel Macs (macOS 12+) and
needs the Claude desktop app.

Builds are not yet signed with an Apple Developer ID, so macOS will refuse to open the app the
usual way. After dragging it to Applications, run this once, then open Hitch normally:

```sh
xattr -cr /Applications/Hitch.app
```

## Develop

Requires Rust, Node 22+ and pnpm.

```sh
pnpm install
pnpm tauri dev      # run the app
pnpm test           # UI and script tests
pnpm test:rust      # core tests
pnpm check          # everything CI runs
pnpm tauri build    # build Hitch.app and a .dmg
```

To try it without changing your real Claude setup:

```sh
HITCH_CLAUDE_DIR=/tmp/hitch-sandbox/Claude-3p pnpm tauri dev
```

## Release

Releases are cut by GitHub Actions.

1. Go to **Actions → Bump version → Run workflow** and choose patch, minor or major
   (or `gh workflow run bump-version.yml -f bump=patch`).
2. That raises the version everywhere, commits it to `main`, tags it `vX.Y.Z`, and starts the
   **Release** workflow.
3. Release runs the full checks, builds a universal macOS `.dmg`, and publishes it on the
   Releases page.

Pushing a `vX.Y.Z` tag by hand also starts a release, as long as the tag matches the version in
the code (`pnpm version check`).

## Learn more

- [`docs/01_project_brief.md`](docs/01_project_brief.md) — what Hitch is and how Claude Desktop's third-party mode works
- [`docs/02_tech_stack.md`](docs/02_tech_stack.md) — the stack, layout and commands
- [`docs/03_decisions.md`](docs/03_decisions.md) — decisions and why
