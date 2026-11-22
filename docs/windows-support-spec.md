---
description: 'Windows requirements and Python application contracts for Oxen Hitch support'
---

## Current contract

Spec layer: Requirements + Structural + Behavioral — the user selected Python, so the Windows app host, backend API, tray, process and config contracts move from Rust/Tauri to Python while the existing React UI remains.

## Claude Code with LiteLLM

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| CONFIGURE | Give Claude Code the active Hitch relay URL and profile token | [mandatory] | L4 |
| SELECT | Pass the stable Hitch alias as Claude Code's model name | [mandatory] | L4 |
| ROUTE | Hitch maps the alias to its LiteLLM model group and streams the reply | [mandatory] | L4 |

- L4 Claude Code settings are separate from Hitch's Claude Desktop profile; configuring one MUST NOT be represented as configuring the other.
- L4 WHEN Claude Code runs from the Windows Claude Desktop Code mode, its process MUST inherit `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN` from the active Hitch profile; user settings alone do not configure an already-running Desktop parent process.
- L4 WHEN Claude Code selects a Hitch model label, its wire model MUST be the matching stable alias; the local profile token MUST authenticate only to Hitch and MUST NOT be presented as a provider credential.
- L4 IF `ANTHROPIC_API_KEY` is configured for the relay, THEN it MUST equal the active Hitch profile token because the relay checks `x-api-key` before bearer authorization.
- L5 Claude may seal `inferenceGatewayApiKey` on disk; Hitch MUST keep the LiteLLM relay token in its credential store and MUST never authenticate with a `__sealed__:v1:` profile value.
- L5 WHEN an existing profile has only a sealed key and no saved relay token, Hitch MUST create a new relay token, save it separately, and write it to the Claude profile so Claude can decrypt it for requests.
- L5 WHEN Claude Desktop seals the profile after applying it, Hitch MUST restore the relay from its credential store and continue accepting the plaintext token Claude sends.
- L6 WHEN LiteLLM routes Claude Messages to an OpenAI-compatible backend that implements chat completions but not the Responses API, its Anthropic adapter MUST use the chat-completions endpoint.
- CLOSED: Direct LiteLLM routing with original model IDs is not used by the embedded Code mode because its invocation supplies Hitch aliases.

## Python module map

| Module | Contract | Inputs and outputs |
|---|---|---|
| `hitch/config.py` | Claude paths, policy precedence, profile library read/write/restore | Windows or injected sandbox paths; JSON profile snapshots |
| `hitch/state.py` | Durable non-secret Hitch settings with atomic writes | State JSON path; `HitchState` values |
| `hitch/secrets.py` | Per-host provider credentials and profile-scoped relay tokens in the Oxen CLI auth TOML | Host/profile key and credential; key hint only leaves the service |
| `hitch/gateway.py` | URL normalization, gateway check, model fetch and test message | httpx transport; typed results |
| `hitch/models.py` | Convert Oxen catalog to Claude model entries | Chosen ids and model catalog |
| `hitch/process.py` | Exact-path Claude Desktop discovery, install check and safe restart status | Windows process/path observations |
| `hitch/service.py` | Compose config, state, secrets, gateway and process operations | Public `Api` contract; status and operation results |
| `hitch/bridge.py` | Expose the `Api` contract and status notifications to pywebview | JSON-compatible JS API values |
| `hitch/app.py` | Window, tray, autostart, open/resize/hide/quit and packaging entry | Built React assets and Windows shell |

The module map is the structural contract; implementation may combine adjacent modules only when it preserves the listed boundaries and test seams.

## Behavioral contracts

- Profile and state writes use temporary sibling files followed by atomic replace; a parse or write failure leaves the previous file intact.
- Profile mutation preserves unknown Claude fields, existing sealed credentials when no replacement key is supplied, profile ordering, and the prior applied profile/deployment mode needed by Remove.
- `HITCH_CLAUDE_DIR` redirects every Claude configuration read/write to its value and disables registry-managed policy checks for sandbox acceptance.
- The Python desktop URL marker MUST be a URL fragment (`#desktop=python`), because pythonnet's `System.Uri` encodes query text on a `file://` URL into the local filename; the React app MUST read the fragment and wait for `pywebviewready` before selecting the Python API and mounting desktop UI.
- `status` is read-only; gateway or process probing failures return truthful error/status values without mutating Claude configuration.
- A connect operation uses a newly supplied key or a key saved for the destination host; it MUST NOT send credentials saved for another host, and it saves a supplied key under the destination host only after a successful profile write and state save.
- Manual custom endpoint mode MUST save the URL, optional endpoint key, and entered IDs without sending an HTTP request. If the key is blank, write a nonsecret placeholder into Claude's gateway key field.
- Manual custom endpoint mode shows `configured, untested` after Save. Both custom modes MUST skip automatic network checks at startup or focus.
- A custom base URL ending in `/v1` MUST normalize so `/v1/models` and `/v1/messages` target that version root without adding `/api/ai`.
- Manual custom model entries preserve the endpoint's exact model ID. LiteLLM discovery uses the alias contract below.
- Existing Oxen aliasing remains unchanged: Oxen's non-Anthropic IDs receive `claude-via-*` aliases resolved by OxenHub. Standard OpenAI Chat Completions endpoints require protocol translation and are outside this config-only change; custom endpoints must accept Claude's Anthropic Messages route.
- While status is read, the service MUST compute `connected`, `paused`, `drifted`, or `not_configured` from persisted ownership plus Claude's current applied profile and mode; status MUST NOT call the gateway.
- When Oxen mode Connect or LiteLLM discovery is requested, the service MUST collect models before config writes. Manual custom endpoint Save performs no network requests.
- When model refresh succeeds, the service MUST retain the catalog in non-secret state and rewrite the connected profile only when its selected model configuration is stale.
- When a profile write succeeds, the bridge MUST broadcast the new status to the open UI and tray state.
- When gateway checks or message probes fail, the service MUST preserve existing Claude files and return a display-safe error.
- pywebview receives only JSON-compatible values and exception messages suitable for UI display; API key material never appears in status, event payloads, or state JSON.
- Unsupported graceful restart reports manual quit/reopen steps and never terminates Claude.
- Packaging tests inject all config, state, secret and process paths; acceptance checks verify the complete output tree, required pythonnet runtime modules, and a visible window loading relative frontend assets before considering a layer passed.

## Execution pipeline

`path/policy discovery -> state and secret load -> config snapshot -> gateway/model operations -> profile mutation -> status/event serialization -> pywebview/tray shell -> PyInstaller bundle`

Each stage is independently testable; later stages cannot be accepted while an earlier contract is open.

## Decision review

- Oracle reviewed pywebview against a native PySide6 rewrite. PySide6 gives native Windows tray and window handling, but requires recreating the existing React screens; pywebview reuses those screens and a WebView2 runtime was found registered on this machine. Keep pywebview unless its real bridge smoke test fails.
- Use `psutil` for exact executable-path and process creation-time observations; process names alone cannot distinguish Claude Desktop from another `claude.exe`.
- Use `tomlkit` for Oxen auth edits so unrelated hosts, default host and comments survive.
- Save endpoint keys under the matching host in the existing Oxen CLI auth TOML; never reuse a key across hosts.
- Windows `os.replace` failure while a destination is open must preserve the original; do not delete-then-replace.
- `chmod(0o600)` does not establish Windows owner-only ACLs. Do not claim key-file ACL parity until a Windows ACL-specific implementation is verified.
- Keep restart manual until a graceful Claude quit method is verified; only launch after the Desktop process is observed stopped.

### Definitions

- :Claude_3p_Directory: is Claude Desktop's local third-party configuration directory for the current Windows user.
- :Claude_Install_Root: is the supported Squirrel installation root containing the stable launcher and versioned app directories.
- :Claude_Main_Process: is a running process whose full executable path belongs to a supported Claude Desktop installation.
- :Managed_Inference: is a Windows registry policy that sets an inference provider or bootstrap URL and overrides Hitch's local profile.
- :Graceful_Restart: is a user-requested quit and reopen that permits Claude to save state and does not force-terminate processes.

### Implementation requirements

- Windows local configuration lives under `%LOCALAPPDATA%\Claude-3p\configLibrary\`; `_meta.json` selects a profile and `<id>.json` stores it.
- Windows managed configuration is read from `HKLM\SOFTWARE\Policies\Claude` and `HKCU\SOFTWARE\Policies\Claude`; machine policy takes precedence and suppresses user policy merging.
- The supported initial Claude installation is the directly observed Squirrel layout under `%LOCALAPPDATA%\AnthropicClaude`; MSIX and Roaming layouts remain unsupported until verified.
- The stable root `claude.exe` is the launch target when present; running-process identity includes versioned `app-*\claude.exe` executables.
- Process matching MUST compare executable paths; process-name-only matching and force termination are forbidden.
- Python owns Claude config, gateway, secrets, app state, process inspection, tray lifecycle and UI bridge operations; React calls only the Python bridge adapter.
- `pywebview` hosts the existing React build and exposes the backend API; bridge readiness uses `pywebviewready`.
- When the Python shell requests the packaged page, it MUST identify desktop mode in the URL fragment so startup never selects the fake browser preview before the bridge is injected and `System.Uri.LocalPath` remains the existing frontend file path.
- `pystray` owns the Windows notification-area icon and menu; its Windows backend is used without process-name matching or force termination.
- PyInstaller builds a Windows application bundle; first validate one-folder mode, then choose a distributable layout.
- The Windows Vite production build MUST use relative asset URLs so the packaged `file://` page loads JavaScript, styles and images from its bundled `dist` directory.
- The PyInstaller spec MUST explicitly collect the dynamically imported `clr` module and pywebview's Python.NET runtime files; successful static analysis alone does not establish pywebview startup.
- Rust/Tauri is not part of the Windows application build path; retain source until Python parity passes, then remove it only after parity acceptance.

### Test requirements

- Test path discovery with missing environment variables, explicit sandbox override, stable launcher present/absent, and multiple versioned app directories.
- Test provider-generic key labels/errors, `/v1` URL normalization, manual model aliases, and rejection of cross-host credential reuse.
- Test manual endpoint Save performs no network calls. Test LiteLLM discovery, aliases, and auth with a simulated catalog and Windows package against the local LiteLLM service.
- Test registry policy with no policy, user inference policy, machine inference policy, app-behavior-only policy and machine/user conflict.
- Test executable identity against the stable launcher, versioned Claude main process, helper executable, and unrelated `claude.exe` CLI.
- Test restart outcomes without terminating a real process: running and not running, quit refusal, delayed open, and unsupported graceful quit.
- Test tray geometry for top, bottom and side taskbars and multiple monitor scale factors.
- Verify Python unit tests, React UI tests/build, one-folder executable launch, sandbox-only config writes, then real Claude integration separately.

### Functional requirements

- Where `HITCH_CLAUDE_DIR` is set, Hitch MUST write Claude configuration only below that directory and MUST clear machine/user managed-source checks.
- When Windows policy sets `inferenceProvider` or `bootstrapUrl`, Hitch MUST report managed configuration and MUST NOT claim its local Oxen profile controls Claude.
- When Windows Claude Desktop is installed in the supported Squirrel layout, Hitch MUST report installation and identify the exact running Desktop executable across versioned `app-*` directories.
  - Given the Claude Code CLI is running from another `claude.exe` path, when Hitch checks Claude status, then the CLI MUST NOT be reported or controlled as Claude Desktop.
- When the user requests restart on Windows and no verified graceful quit mechanism is available, Hitch MUST give manual quit/reopen instructions and MUST NOT force-terminate Claude.
- When Hitch opens from the Windows notification area, its window MUST remain within the work area of the monitor containing the tray icon, above or beside the taskbar as appropriate.
- When building for Windows, PyInstaller MUST produce a launchable bundle without requiring Rust or Tauri.
- Given a disposable existing Claude profile, when Hitch connects and is removed, then the prior applied profile and deployment mode MUST be restored.

### Current implementation mapping

| Contract | Current source | Disposition |
|---|---|---|
| Windows config path | `hitch/config.py` | Implemented; fixture tests pass |
| Policy detection | `hitch/config.py` | Implemented; policy precedence fixture tests pass |
| Process identification | `hitch/windows.py` | Implemented; exact path and creation-time fixtures pass |
| Graceful restart | `hitch/windows.py` | Implemented as a truthful manual flow; no process termination |
| Windows tray lifecycle | `hitch/app.py` | Implemented; interactive menu and work-area placement acceptance remain open |
| Windows packaging | `OxenHitch.spec`, `package.json` | Implemented; PyInstaller onedir includes pythonnet and relative Vite assets; launch and UI render pass |
| UI bridge | `hitch/bridge.py`, `src/lib/api.ts`, `src/main.tsx` | Implemented; fragment URL tests pass; rebuilt package rendered React UI and returned Python `status()` over CDP |

### Provider endpoint requirements

#### Windows LiteLLM discovery and aliases

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| DISCOVER | Read model IDs from the supplied LiteLLM URL when Save has no manual IDs | [mandatory] | L1 |
| MAP | Derive a stable `claude-model-<hex>` alias from each upstream ID | [mandatory] | L2 |
| APPLY | Save the alias list and a loopback gateway in Claude's profile | [mandatory] | L2 |
| ROUTE | Replace the alias and stream the Anthropic response from LiteLLM | [mandatory] | L3 |

- L1 WHEN the user saves with blank model names, Hitch MUST read that endpoint's `/v1/models` using the optional provider key; if LiteLLM supplies model metadata, omit groups explicitly marked non-chat.
- L1 IF the catalog fails or contains no usable models, THEN Hitch MUST report the error without applying a new profile. Manual IDs retain the offline direct configuration path. Startup and focus MUST NOT trigger discovery.
- L2 WHEN Hitch creates an alias, Claude MUST receive it as the wire model name and the relay MUST map that exact alias to the original LiteLLM model ID. The configured loopback address and mapping survive restarting Hitch.
- L2 Claude's picker and the local model catalog MUST retain LiteLLM's display name or fall back to the original LiteLLM ID. On reopening Hitch, older profiles with aliases shown as labels are corrected from the cached catalog without changing IDs, order or active profile selection.
- L3 WHEN Claude uses the alias gateway, Hitch MUST authenticate the local request, accept only known aliases, and forward Messages and token count requests to the chosen upstream. LiteLLM handles protocol conversion.
- CLOSED: A picker label can't change the model on the wire. Updating LiteLLM's configuration would require management access and alter the shared proxy.

- The host and credential are provider-neutral; the user supplies the API key expected by the configured provider, not an Oxen-specific key.
- Manual custom endpoint Save uses the exact IDs the user supplied and reports `configured, untested`. An absent provider key is valid in either custom mode.
- Claude Desktop sends Anthropic Messages requests to `/v1/messages`. An endpoint that implements only OpenAI Chat Completions is outside this contract unless a separate protocol adapter is added.

## Python dependency contract

Pinned Windows runtime and build dependencies are recorded in `requirements.txt`, including pythonnet and clr_loader because pywebview loads its WebView2 backend dynamically. The Windows React build uses `pnpm build:windows` to write relative asset URLs for the packaged file page; the package manager version is pinned in `package.json`.

### Validation

- The config path and registry policy semantics are grounded in Anthropic's configuration implementation: https://claude.com/docs/third-party/claude-desktop/configuration-format
- The prior Rust compile was verified before the user selected Python; it is historical evidence only and does not validate this implementation or the Python build.
- The Windows implementation reuses `src/` React UI through pywebview and hosts gateway behavior in Python.
- pywebview supports a Python JS API bridge and local static content; pystray documents a Windows tray backend; PyInstaller builds an OS-specific executable and recommends validating one-folder mode first.
- The current supported install scope is the local Squirrel layout observed during Windows inspection; this does not establish MSIX behavior.
- Python dependency versions are pinned in `requirements.txt`; the verified venv currently uses pytest 8.4.2 because pytest 9.2.0 became internally inconsistent after an interrupted package replacement.
- Packaged launch verification caught missing dynamic `clr` collection and Vite root-absolute asset URLs. The later user screenshot disproved the previous startup acceptance: a query marker on the `file://` URL becomes part of the local filename in pywebview's WebView2 backend. Use only `#desktop=python`; require a real packaged UI render before accepting startup.
- Runtime direct-write acceptance remains open until a disposable Windows user profile can be used without touching the primary Claude profile.
- The previous Rust/Tauri implementation remains the behavior oracle until Python parity tests and sandbox acceptance pass.
