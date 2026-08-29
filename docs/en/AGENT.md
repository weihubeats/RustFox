# AI Agent Integration Guide

**Language / 语言**：[简体中文](../AGENT.md) · English

RustFox ships with a built-in **Agent control plane**: when the desktop app starts it also starts a local HTTP service (loopback only, `127.0.0.1`) so AI agents can save cURL commands straight into RustFox as endpoints, and query projects and endpoints.

Two integration options:

| Option | Applies to | Prerequisite |
| --- | --- | --- |
| MCP server (recommended) | MCP-capable clients such as Claude Code, Cursor | The `rustfox-mcp` binary |
| Direct HTTP | Any agent that can send HTTP / run commands | None |

---

## 1. How it works

```
AI Agent ──(stdio MCP)──→ rustfox-mcp ──┐
                                        ├──→ Agent control plane (127.0.0.1:4110~4129)
AI Agent ──(HTTP + Bearer token)────────┘         │
                                             SQLite → UI refreshes live
```

- The control plane **starts automatically with the RustFox desktop app**; nothing to enable manually.
- The port auto-probes from `4110` (avoiding the Mock range around 4010).
- Every request must carry a token: `Authorization: Bearer <token>` or `X-Agent-Token`.

## 2. Find your token

The token is the control-plane credential, generated automatically on first app start:

- File location: `{data dir}/agent-token` (permissions 0600)
- Data directory:
  - Windows: `%APPDATA%\RustFox\agent-token`
  - Linux: `~/.local/share/RustFox/agent-token`
- You can also confirm it in-app: open DevTools Console and run
  `await __TAURI_INTERNALS__.invoke('plugin:fox|agent_status')` to see `tokenPath`.

> Development builds (`tauri dev`) use the `RustFox-dev` directory, isolated from the release install.

## 3. Option one: MCP server

### 3.1 Get rustfox-mcp

**Since v0.0.10 the installer bundles `rustfox-mcp`** — use the per-platform path:

| Platform / install | Path |
| --- | --- |
| macOS (/Applications install) | `/Applications/RustFox.app/Contents/MacOS/rustfox-mcp` |
| Windows (NSIS default) | `C:\Program Files\RustFox\rustfox-mcp.exe` |
| Linux (.deb) | `/usr/bin/rustfox-mcp` (already on PATH — configure as `rustfox-mcp`) |
| Linux (.AppImage) | `rustfox-mcp` inside the mounted image |
| Dev mode (tauri dev) | `frontend/src-tauri/binaries/rustfox-mcp-<triple>` in the repo |

<details>
<summary>Older versions (&lt; v0.0.10) or building from source</summary>

```bash
git clone https://github.com/weihubeats/RustFox.git
cd RustFox && cargo build --release -p fox-mcp
# Output: target/release/rustfox-mcp
sudo cp target/release/rustfox-mcp /usr/local/bin/   # optional: put on PATH
```
</details>

### 3.2 Configure your client

For v0.0.10+ installs the binary is **not on PATH** — prefer the absolute paths from §3.1 (classic macOS pitfall: a bare `rustfox-mcp` fails with `spawn ... ENOENT`). A bare command works only once it is on PATH.

**Claude Code** — project root `.mcp.json`:

```json
{
  "mcpServers": {
    "rustfox": { "command": "/Applications/RustFox.app/Contents/MacOS/rustfox-mcp" }
  }
}
```

In dev mode (tauri dev), or once on PATH:

```json
{ "command": "rustfox-mcp" }
```

**Cursor** — Settings → MCP → Add Server; set Command to the absolute path `/Applications/RustFox.app/Contents/MacOS/rustfox-mcp` (macOS) or your platform's path.

Restart the client / reload the session; when the tool list shows 4 tools you're done:

| Tool | Description |
| --- | --- |
| `save_curl` | Parse a cURL command and save it as an endpoint (returns endpointId) |
| `list_projects` | List projects |
| `list_endpoints` | List endpoints of a project (needs projectId) |
| `agent_info` | Control-plane address and token path |

Then just say "save this curl to RustFox" in chat; on success the desktop sidebar refreshes live and shows a toast.

### 3.3 save_curl parameters

| Parameter | Required | Description |
| --- | --- | --- |
| command | ✓ | The full cURL command string |
| name | | Endpoint name; derived from the last URL path segment when omitted |
| projectId | | Target project; with a single project it is auto-selected, with zero projects an "Agent imports" project is created, with multiple projects the call errors and lists candidates |
| folderId | | Destination folder |

Import behavior:

- The URL is split into base_url + path + query params (same as manual cURL import); the endpoint lands as a "Designing" draft.
- If the `base_url` project variable is missing, the URL origin is written automatically.
- **A different existing base_url is never overwritten** — the response carries a `warning` field, which the agent will relay.

### 3.4 Examples

**① One-liner import from an existing cURL** (Claude Code / Cursor):

```
Save this curl to RustFox:
curl -X POST https://api.example.com/orders \
  -H "Content-Type: application/json" \
  -d '{"userId":1,"amount":99}'
```

**② Save an endpoint straight from Java code**: paste the Controller code (or select the method in Cursor) and the AI builds the cURL and calls `save_curl`:

```
Save this endpoint to RustFox:
@PostMapping("/orders")
public Result<Long> createOrder(@RequestBody CreateOrderReq req) {
    return orderService.create(req);
}
```

**③ Target a specific project / folder**: call `list_projects` first, then import with the `projectId`:

```
Find the projectId via list_projects, then save_curl:
curl -X POST http://127.0.0.1:8080/api/users -d '{"name":"foo"}' name="Create user" projectId=<projectId>
```

With multiple projects, omitting `projectId` makes `save_curl` fail and list candidates — copy one and retry.

## 4. Option two: direct HTTP

For custom scripts or agents without MCP support.

| Method | Path | Description |
| --- | --- | --- |
| POST | `/agent/curl` | `{command, projectId?, name?, folderId?}` → `{endpointId, ...}` |
| GET | `/agent/projects` | List projects |
| GET | `/agent/endpoints/:projectId` | List endpoints |
| GET | `/agent/health` | Liveness probe |

Example:

```bash
TOKEN=$(cat "$HOME/Library/Application Support/RustFox/agent-token")
curl -s http://127.0.0.1:4110/agent/curl \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"command":"curl -X POST -H \"Content-Type: application/json\" -d \"{\\\"a\\\":1}\" https://api.example.com/orders"}'
```

Fields accept both camelCase and snake_case (`projectId` / `project_id`).
Errors are always `{code, message}`: `VALIDATION`(400) / `NOT_FOUND`(404) / `UNAUTHORIZED`(401).

## 5. Security design

- Binds to `127.0.0.1` only — unreachable from outside.
- Random UUID token with 0600 file permissions.
- The only write operation is "import an endpoint"; existing data cannot be deleted or modified.
- Encrypted environment variable plaintext in the keychain is never read.

## 6. Troubleshooting

| Symptom | Fix |
| --- | --- |
| `spawn rustfox-mcp ENOENT` | Binary not on PATH: for v0.0.10+ use the absolute path from §3.1, or build the old way and put it on PATH |
| `rustfox-mcp` says "no running control plane" | Start the RustFox desktop app first; make sure the local loopback 4110~4129 is not blocked by a firewall |
| 401 UNAUTHORIZED | The token file belongs to a different install (e.g. mixing dev/release directories); delete `agent-token` and restart the app, then reconfigure |
| VALIDATION with multiple projects | Have the agent call `list_projects` first and retry with a projectId |
| MCP tool list missing | Check `.mcp.json` syntax and the execute permission on rustfox-mcp (`chmod +x`); check the client's MCP logs |
| UI doesn't refresh after import | Live refresh only happens when the desktop app has the same project open; switch projects or re-enter the workspace to see it |
