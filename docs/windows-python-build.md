# Build Oxen Hitch for Windows with Python

## Current status

Claude Desktop 6.28899.6 showed all 17 original LiteLLM names in the expanded picker after a full tray **Exit** and relaunch. Opening the launcher while Claude still runs in the tray reuses the existing instance and retains its earlier picker labels.

To use LiteLLM, choose **Use a different endpoint**, keep `http://127.0.0.1:4000`, and click **Save endpoint** with model names blank. Claude's picker shows the original LiteLLM display names. Hitch uses stable `claude-model-<hash>` IDs for routing and repairs the labels in earlier Hitch profiles on startup. Hitch stays in the Windows notification area to route those aliases while Claude is open. Quit stops the relay. Use **Models → Refresh models** when LiteLLM's catalog changes.

### Add the local Qwen3-VL models to LiteLLM

The Qwen3-VL 2B and 4B GGUF models are served by Ollama at `http://127.0.0.1:11434/v1`. Add these deployments to the existing LiteLLM `model_list`; keep any other model and role entries already in that file:

```yaml
model_list:
  - model_name: qwen3-vl-2b
    litellm_params:
      model: openai/hf.co/Qwen/Qwen3-VL-2B-Instruct-GGUF:Q4_K_M
      api_base: http://127.0.0.1:11434/v1
      api_key: ollama
    model_info:
      supports_vision: true
      max_input_tokens: 131072

  - model_name: qwen3-vl-4b
    litellm_params:
      model: openai/hf.co/Qwen/Qwen3-VL-4B-Instruct-GGUF:Q4_K_M
      api_base: http://127.0.0.1:11434/v1
      api_key: ollama
    model_info:
      supports_vision: true
      max_input_tokens: 131072

  - model_name: party-oracle-vision
    litellm_params:
      model: openai/hf.co/Qwen/Qwen3-VL-4B-Instruct-GGUF:Q4_K_M
      api_base: http://127.0.0.1:11434/v1
      api_key: ollama
    model_info:
      supports_vision: true
      max_input_tokens: 131072
```

`api_key: ollama` is a placeholder for an unauthenticated local Ollama server; use the credential configured on your Ollama host if it requires authentication. Do not replace the existing `party-oracle` route: `party-oracle-vision` is an explicit 4B vision-capable role.

After editing the config, restart LiteLLM so it loads the routes. Verify that `GET http://127.0.0.1:4000/v1/models` lists all three names, then click **Models → Refresh models** in Hitch. In Claude Code, select **party-oracle-vision** when sending images; Hitch keeps its stable Claude wire alias and routes it back to this LiteLLM name. `party-oracle` remains the existing text reasoning route.

The installed Ollama tags report vision capability and a 262144-token native context. LiteLLM advertises a 131072-token input limit here. On 2026-11-22, the running service listed all three aliases at `/v1/models`; three image prompts through `/v1/chat/completions` and one through the Anthropic `/v1/messages` path returned HTTP 200 with non-empty answers from `party-oracle-vision`, and a separate image request to `qwen3-vl-2b` returned HTTP 200 with a non-empty answer through `/v1/messages`. Direct Ollama image probes also produced usable answers from both model sizes. Selecting the route in Claude Code through Hitch remains the final UI acceptance check.

### Claude Code in Claude Desktop on Windows

Claude Desktop launches its Code mode as a bundled Claude Code process. That process uses Hitch's hashed wire model alias, so it must reach Hitch's loopback relay rather than LiteLLM port 4000 directly. For this machine, the active relay is `http://127.0.0.1:52338`; Hitch's profile key is the local relay token. `ANTHROPIC_API_KEY` must contain that exact local token when present; an upstream provider key is rejected by the relay. Hitch checks `x-api-key` before bearer authorization.

The Claude Code user settings and Windows user environment must use the active relay URL, profile token, and alias for the desired model. Preserve unrelated settings and environment values. If a project `.env` is used, its filename must be `.env` (Windows may append `.txt`) and its `ANTHROPIC_API_KEY` must match the Hitch profile token. Restart Claude Desktop after changing the Windows user environment so its Code process inherits the relay values. Hitch's ordinary Claude Desktop profile remains unchanged.

Validation: Claude Code 2.1.330's bundled Windows binary completed a one-turn request through the active Hitch relay using the qwen alias and returned `OK`; Claude Desktop was restarted with the relay environment.

Entering model names manually still saves a direct Anthropic Messages gateway profile offline. The Windows backend and desktop host use Python. The Python suite passes 23 tests, the React suite passes 62 tests, and the Windows frontend build uses relative asset URLs.

Packaged acceptance discovered 17 chat model groups from the running LiteLLM on port 4000 with both the key and model fields blank. The package restored the same alias gateway after restart and returned a live token count through a generated alias.

The bundled app previously showed a responding window but the user captured WebView2 `ERR_FILE_NOT_FOUND`. The cause was `?desktop=python` on a `file://` URL: pythonnet treats the query as part of the local filename. The host and React app now use `#desktop=python`; CDP acceptance confirmed the rebuilt bundle renders React and returns isolated status through the Python bridge. A disposable real Claude profile, live gateway credentials, tray-menu interaction and autostart acceptance remain unverified.

## Requirements

- Windows 10 or 11 with Microsoft Edge WebView2 Runtime.
- Python 3.12 (64-bit). Python 3.10.0 has a CPython bytecode disassembly bug that makes PyInstaller fail while analyzing Bottle, a pywebview dependency; Python 3.12 passes the same source scan.
- Node.js and pnpm 11.25.0.

## Setup and build

Run these commands from the repository root in PowerShell:

```powershell
py -3.12 -m venv .venv312
.\.venv312\Scripts\python.exe -m pip install -r requirements.txt
pnpm install --frozen-lockfile
pnpm build:windows
.\.venv312\Scripts\python.exe -m pytest tests -q
.\.venv312\Scripts\python.exe -m PyInstaller --noconfirm --clean --distpath dist-python --workpath build-python OxenHitch.spec
```

The application bundle is `dist-python/OxenHitch/OxenHitch.exe`; distribute the full `OxenHitch` folder, not the executable alone. The build uses Python, Node/pnpm and PyInstaller; it does not invoke Cargo or Tauri.

## Sandbox launch

Use a fresh temporary data directory to avoid changing Claude's real profile:

```powershell
$env:HITCH_CLAUDE_DIR = Join-Path $env:TEMP 'OxenHitch-Python-Smoke'
$env:HITCH_BASE_URL = 'http://127.0.0.1:9'
New-Item -ItemType Directory -Force $env:HITCH_CLAUDE_DIR | Out-Null
.\dist-python\OxenHitch\OxenHitch.exe
```

Clear the two environment variables after the smoke run. Confirm that the window appears, use Show and Quit from the tray menu, and check that all profile, state and auth writes stay under `HITCH_CLAUDE_DIR` (the auth file is isolated beside the sandbox Claude directory).

## Validation record

- Python 3.12 Bottle bytecode scan: passed; 1,393 module instructions and recursive code objects were disassembled without error.
- Python suite on Python 3.12.0: 17 passed.
- PyInstaller 6.22.3: onedir build completed successfully on Windows 11.
- Bundled pythonnet runtime: archive contains the dynamically loaded `clr` module and `Python.Runtime.dll`.
- Bundled startup: CDP observed `file:///.../_internal/dist/index.html#desktop=python`, the rendered `Run Claude on Oxen` screen, `window.pywebview.api` as an object, and `status()` returning `link: not_configured` with sandbox `baseUrl: http://127.0.0.1:9`. A window capture is saved at `%TEMP%\oxenhitch-current-package.png`; the process closed cleanly.
- Frontend: 60 Vitest tests passed and `pnpm build:windows` completed with relative asset URLs.
- Custom endpoint acceptance: React tests verify Save does not trigger the background check; Python tests verify no gateway call, no required key, exact model IDs, and model discovery disabled.
- Earlier source-host pywebview sandbox smoke: WebView2 window opened and closed cleanly.
- Packaged checks caught PyInstaller omitting dynamic `clr`, Vite emitting root-absolute asset URLs, and the `file://` query marker becoming part of the local filename. The spec includes `clr`, the Windows build emits relative URLs, and Python desktop mode uses a URL fragment and waits for `pywebviewready`. Exact rebuilt bundle acceptance passes.
- Real Claude profile, Oxen gateway, tray-menu interaction and autostart acceptance: open.

## Why Python 3.12

The initial package analysis on Python 3.10.0 failed inside `dis.get_instructions()` on Bottle with `IndexError: tuple index out of range`. The issue is tracked as [CPython bpo-45757](https://bugs.python.org/issue45757). Re-running the same scan on the installed Python 3.12 runtime succeeded, and PyInstaller then completed without suppressing analysis errors or changing Bottle.

## References

- [pywebview usage and JavaScript API](https://pywebview.idepy.com/en/guide/usage)
- [pystray usage](https://pystray.readthedocs.io/en/stable/usage.html)
- [PyInstaller operating modes](https://www.pyinstaller.org/en/stable/operating-mode.html)
- [Claude configuration format](https://claude.com/docs/third-party/claude-desktop/configuration-format)
