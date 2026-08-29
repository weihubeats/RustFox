# RustFox

> A lightweight cross-platform API debugging client in ~10 MB. One installer, ready to run.

**Language / 语言**：[简体中文](README.md) · English

<!--
  Screenshots: docs/imags/home.png (dashboard), docs/imags/api-home.png (workspace).
  Replace the paths below after updating the screenshots.
-->
[![RustFox dashboard](docs/imags/home.png)](docs/imags/home.png)

[![RustFox workspace](docs/imags/api-home.png)](docs/imags/api-home.png)

## Why RustFox?

### 🪶 10 MB, zero runtime dependencies

Competing tools bundle an entire Chromium/Node.js runtime. RustFox is built with **Rust + Tauri 2 + the system WebView** — no bundled browser engine, no Node sandbox, no JRE. Install and go.

| Dimension | RustFox | Postman | Bruno | Insomnia |
| --- | --- | --- | --- | --- |
| Installer size | **~10 MB** | ~310 MB (Electron) | ~433 MB (Electron) | ~200 MB (Electron) |
| Cold start | **< 1 s** | 2–4 s | 2–5 s | 2–4 s |
| Runtime memory | **~40 MB** | ~500 MB+ | ~300 MB+ | ~200 MB+ |
| App shell | System WebView (no bundled Chromium) | Chromium + Node.js | Chromium + Node.js | Chromium + Node.js |

Same workload: **20–40× smaller** installer, **2–5× faster** startup, an order of magnitude less memory.

### ⚡ Fast, without cutting features

Rust LTO + a single-process model + zero-copy SQLite storage — instant open, search, and send. Requests / Mock / testing / load testing are all built in, with no cloud dependency.

### 🔒 Local-first, your data stays yours

Everything lives in a local `rustfox.db`; environment variable values are **AES-256-GCM encrypted** at rest. One-click JSON backup, restore anytime.

## Features

### Requests & editing

- 8 HTTP methods, 6 body types (JSON / Form / x-www-form-urlencoded / Multipart / GraphQL / Text)
- Params / Headers / Body / Auth / Tests / Docs tabs; unsaved drafts are flagged automatically
- Paste-to-import cURL: method / URL / headers / body / Basic Auth detected automatically
- `{{name}}` variables resolved anywhere (environment > project precedence)

### Auth & security

- API Key / Basic / Bearer / OAuth2 (Authorization Code / Client Credentials / Password / Implicit)
- Tokens attach to requests automatically — no hand-written headers

### Response experience

- Pretty JSON tree / raw / response headers / status / duration / size
- Streaming downloads to disk; request history can be re-sent or deleted

### Mock server

- Local axum mock server (port auto-probing from 4010), works fully offline
- **Mock rules** (method + path + header + body matching) take priority; endpoint "response examples" as fallback
- Template variables: `{{params.id}}` `{{headers.X-Token}}` `{{mock.uuid|email|name|word|int}}`

### Automated testing & load testing

- JSON test scripts: `pre_request` variable injection, `extract` capture, `assertions`
- Run one endpoint / a folder / the whole project; results and history are kept
- Load testing: concurrency × total requests, with QPS, average latency, P50/P90/P99 and error samples charted via chart.js

### Import / export & collaboration

- OpenAPI 3.x / Swagger 2.0 / Postman Collection v2.1 import and export
- Markdown docs for a single endpoint or the whole project
- Client code generation: cURL / JavaScript / Java / Go / Rust / Python / PHP (variable substitution and auth headers included, syntax highlighted)

### More

- GraphQL debugging view
- Backup (JSON) & restore (full ID remapping, never overwrites existing data)
- Dark / light / follow-system theme

## AI Agent integration

RustFox ships with a built-in **Agent control plane** (started automatically with the app): a token-protected HTTP API on loopback that lets AI agents (Claude / Cursor / anything that can run commands) save cURL commands as endpoints — no manual copy-paste.

Full guide (`rustfox-mcp` setup, client configuration, HTTP API reference, troubleshooting): **[docs/en/AGENT.md](docs/en/AGENT.md)**

### MCP server (recommended)

> **Prerequisite**: MCP needs the `rustfox-mcp` binary. Installers since v0.0.10 bundle it, but it is not on PATH — use the absolute path inside the install (macOS: `/Applications/RustFox.app/Contents/MacOS/rustfox-mcp`).

For MCP-capable clients such as Claude Code, configure it once in the project `.mcp.json`:

```json
{ "mcpServers": { "rustfox": { "command": "/Applications/RustFox.app/Contents/MacOS/rustfox-mcp" } } }
```

> A bare `rustfox-mcp` works only if the binary is on PATH (Linux .deb install, or a `cargo build --release -p fox-mcp` copied into `/usr/local/bin`).

Then just ask the AI to save endpoints into RustFox (paste cURL or source code):

```
Save this endpoint to RustFox:
@PostMapping("/orders")
public Result<Long> createOrder(@RequestBody CreateOrderReq req) { ... }
```

Afterwards, say "save this curl to RustFox" in chat. Four tools are provided:

| Tool | Description |
| --- | --- |
| `save_curl` | Parse a cURL command and save it as an endpoint (URL split into base_url + path + query) |
| `list_projects` | List projects |
| `list_endpoints` | List endpoints of a project |
| `agent_info` | Control-plane address and token file location |

### Direct HTTP

Any tool that can send HTTP can call the control plane directly:

| Method | Path | Description |
| --- | --- | --- |
| POST | `/agent/curl` | `{command, projectId?, name?, folderId?}` → endpointId |
| GET | `/agent/projects` | List projects |
| GET | `/agent/endpoints/:projectId` | List endpoints |
| GET | `/agent/health` | Liveness probe |

Auth: header `Authorization: Bearer <token>` or `X-Agent-Token`;
the token lives in the `agent-token` file inside the data directory (0600). The port auto-probes from `4110`.

Security: loopback-only binding; the only write operation is import; existing `base_url` values are never overwritten (a conflict returns a warning).

## Download & install

Grab the installer for your platform from [Releases](https://github.com/weihubeats/RustFox/releases):

| Platform | Installer |
| --- | --- |
| Windows | `RustFox_*-x64-setup.exe` (NSIS; on SmartScreen pick "More info → Run anyway") |
| macOS | `RustFox_*-aarch64.dmg` (Apple Silicon) / `RustFox_*-x64.dmg` (Intel) |
| Linux | `.deb` / `.rpm` / `.AppImage` |

> **macOS says the app is damaged?** It isn't — that's Gatekeeper blocking an un-notarized app. Move the app into Applications, then run once:
>
> ```bash
> xattr -cr /Applications/RustFox.app
> ```
>
> Then right-click → Open. See the [user guide](docs/en/USER_GUIDE.md#12-macos).

After installing, use "About → Check for Updates" in-app to upgrade (auto-update supported since v0.0.3).

## Building from source (developers)

Prerequisites: Rust toolchain + Node 22.

```bash
cargo build --workspace        # build all backend crates
cargo test --workspace         # run all tests
npm --prefix frontend install
npm --prefix frontend run tauri dev     # dev mode (Vite HMR)
scripts/package-tauri.sh                # package distributables in one step
```

> Architecture, crate layout, IPC and data flow: **[docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md)** (Tauri 2 + Vue 3, with diagrams).

## Documentation

| Document | Description |
| --- | --- |
| [docs/en/ARCHITECTURE.md](docs/en/ARCHITECTURE.md) | Architecture overview (Tauri 2 + Vue 3, with diagrams) |
| [docs/en/USER_GUIDE.md](docs/en/USER_GUIDE.md) | End-user manual |
| [docs/en/AGENT.md](docs/en/AGENT.md) | AI Agent integration (MCP / HTTP control plane) |
| [docs/SPEC.md](docs/SPEC.md) | Detailed spec (Chinese: models / database / commands) |
| [docs/SMOKE_TEST.md](docs/SMOKE_TEST.md) | Manual acceptance checklist (Chinese) |
| [docs/DEPLOY.md](docs/DEPLOY.md) | Release & deployment (Chinese) |
| [docs/MILESTONES.md](docs/MILESTONES.md) | Milestones (Chinese) |
| [docs/PROGRESS.md](docs/PROGRESS.md) | Development progress log (Chinese) |

## License

[Apache-2.0](LICENSE)
