# RustFox User Guide

**Language / 语言**：[简体中文](../USER_GUIDE.md) · English

For end users. **No Rust, no command-line knowledge required** — download, install, double-click.

- Applies to: RustFox 0.1.x
- Platforms: Windows 10/11, macOS 11+, Linux (common desktop environments)

---

## 1. Install & launch

### 1.1 Windows

| Method | Steps |
| --- | --- |
| Installer (recommended) | Download `RustFox-<version>-setup.exe` → double-click → follow the prompts → a **RustFox** shortcut appears on the desktop and a "RustFox" group in the Start menu |

> If Windows SmartScreen says "Windows protected your PC" on first run, click "More info" → "Run anyway". The installer does not carry a code-signing certificate yet — this is expected.

### 1.2 macOS

1. Download `RustFox-<version>-<arch>.dmg` (`aarch64` for Apple Silicon, `x64` for Intel)
2. Mount it and drag **RustFox.app** into Applications
3. **Remove the quarantine flag** (important): the app is not notarized by Apple, so newer macOS may say
   "RustFox is damaged and can't be opened. You should move it to the Trash." — **the app is not damaged**;
   it's Gatekeeper blocking un-notarized apps. Run in Terminal:

   ```bash
   xattr -cr /Applications/RustFox.app
   ```

   (Alternatively: right-click RustFox.app in Applications → Get Info → check "Override Malware Protection".)
4. First launch: right-click RustFox.app → Open, or System Settings → Privacy & Security → Open Anyway — again a normal unsigned-app prompt

Afterwards launch from Launchpad or Applications, or pin it to the Dock.

> In-app auto update ("About → Check for Updates") is unaffected: after the updater replaces the app, the quarantine flag is not re-applied.

### 1.3 Linux

1. Download `rustfox-<version>-amd64.deb` (Debian/Ubuntu) or run the `.AppImage` directly
2. Debian-based install: `sudo apt install ./rustfox-<version>-amd64.deb`

> Linux relies on the system WebKit/GTK libraries (webkit2gtk-4.1, gtk-3). If startup complains about missing `libwebkit2gtk`, install the packages for your distribution.

### 1.4 First launch

The local database initializes automatically on first start (no network, no account) and you land on the home screen:

```
┌─────────────────────────────── RustFox ───────────────────────────────┐
│ RustFox │ [Project ▼] │ [No environment ▼] │  Search │ Feedback │ ⚙   │
├─────────────────────────────────────────────────────────────────────────┤
│                          🦊  Get started with RustFox                    │
│                   Create your first project and start managing APIs      │
│                          [ Create project ]                              │
└─────────────────────────────────────────────────────────────────────────┘
```

## 2. UI overview

| Area | Purpose |
| --- | --- |
| Top bar | Project tabs, environment selector, Mock status, docs/Mock menus, settings |
| Dashboard | Welcome & stat cards, recent activity, quick actions, project cards, drag-and-drop import |
| Workspace | Sidebar tree + tab bar + endpoint editor + response panel |
| Settings | Environment variables, OpenAPI import/export, Mock server, backup & restore |

Dashboard layout:

![RustFox dashboard layout](../imags/home.png)

Workspace layout:

![RustFox workspace layout](../imags/api-home.png)

## 3. Projects

- On the dashboard click "Create project", then enter a name and base URL (`base_url`, e.g. `https://api.example.com`).
- Inside a project the sidebar shows the folder tree: folders and endpoints in a hierarchy; the top bar switches projects.
- All project data stays on your machine (see section 10); back up / restore anytime from Settings.

## 4. Writing & sending requests (core)

Workspace layout:

```
┌─ Project tree ─┬───────────────────────────────────────────────────────┐
│ ▾ my-app      │  [Tab1 ●] [Tab2]  + New                               │
│  ▾ Users       │  [GET ▼]  https://api.example.com/users/{id}  [Save][Send]│
│    GET List    │  ┌ Params │ Headers │ Body │ Auth │ Tests │ Docs ───┐ │
│    POST Create │  │ key        value                                  │ │
└────────────────┴─┴──────────────────────────────────────────────────┴─┘
```

| Step | How |
| --- | --- |
| Open an endpoint | Click it in the sidebar → opens as a tab (clicking an open tab activates it) |
| New endpoint | The `+` button in the sidebar toolbar, or "New request" from a folder's `⋯` menu |
| Edit params | **Params** tab adds query parameters; **Headers** adds request headers (enable toggles whether each is sent) |
| Edit body | **Body** tab: none / form-data / urlencoded / raw (JSON, text…) / binary / GraphQL; JSON mode has pretty/minify/copy actions |
| Auth | **Auth** tab picks the scheme (None / Basic / Bearer / API Key / OAuth2) |
| Save | Click "Save"; unsaved changes are flagged with **●** on the tab title, and switching tabs never loses drafts |

**Send**: click "Send" — the right pane shows status, duration, size, headers and body; recent requests go into "History" (below the address bar).

> Multiple tabs can edit different endpoints at once, each with its own draft; closing an unsaved tab asks first.

### 4.1 Variables

`{{variable}}` works anywhere in a request:

- `{{base_url}}`, `{{token}}` etc. come from the active environment
- Project variables are configured in Settings → project variables
- Substitution happens automatically on send / code generation / load testing

## 5. Environments

Settings → "Environments":

- "New environment": enter a name and variables (key/value pairs).
- Switch environments from the top bar; variables are substituted automatically on send.
- ⚠️ Environment variable values are **encrypted at rest** (AES-256-GCM). **Do not delete** `master.key` in the data directory — encrypted values become undecryptable without it (plaintext can be recovered from a backup JSON).

## 6. Automated testing

Workspace **Tests** tab: attach a JSON test script to an endpoint (pre-request variables, response capture, assertions). Supports:

- `pre_request`: inject variables before the request
- `extract`: capture values from the response and pass them downstream (in folder order)
- `assertions`: status code / body contains / JSONPath value / response duration and more

After saving click "Run tests" and choose **this endpoint / this folder / the whole project**; results are stored in test history (last 20 runs, expandable and deletable).

### Load testing (concurrency benchmark)

In the "Load test" area of the Tests page, enter concurrency and total requests (default 10 × 100) → "Start". Results: success/failure counts, total time, QPS, average latency, P50/P90/P99 percentiles, and up to 5 error samples.

## 7. Client code generation

"Export code" in the address bar → pick a language:

- **cURL** / **JavaScript (fetch)** / **Java** / **Go (net/http)** / **Rust** (Python / PHP also available)

Output is the **fully rendered** request (variables substituted, auth headers attached, enabled headers included) with syntax highlighting — copy it straight into your project.

## 8. Mock server (local API simulation)

Settings → "Mock Server":

```
Settings
  ┌ Mock Server ─────────────────────────────┐
  │ Start Mock  (listens on 4010, +1 if busy) │
  │ Custom mock rules (optional, beat examples)│
  └──────────────────────────────────────────┘
```

- After starting it listens on `http://127.0.0.1:4010` (auto-increments 4010→4001→… until a free port is found).
- Without rules, it serves the endpoint's saved "response examples" (responses stored on the Docs tab).

Response routing / body template variables: `{{params.id}}`, `{{query.name}}`, `{{headers.X-Token}}`, `{{mock.uuid|email|name|word|timestamp|int}}`.
- **Restart the Mock server (stop, then start) after changing endpoints or rules.**

## 9. Backup & restore / doc export

| Feature | Where | Notes |
| --- | --- | --- |
| Back up a project | Settings → "Back up project" | Exports all endpoints, environments, mock rules and examples as JSON into `backups/` |
| Restore | Settings → paste JSON → "Restore" | Creates a **brand-new project** (all IDs remapped, never overwrites existing data) |
| Export docs | Workspace Docs tab (on an open endpoint) | Generates project Markdown docs into `exports/` |

## 10. Where is my data?

Everything lives under the system data directory / RustFox:

| Platform | Path |
| --- | --- |
| Linux | `~/.local/share/RustFox/` |
| macOS | `~/Library/Application Support/RustFox/` |
| Windows | `%APPDATA%\RustFox\` |

- `rustfox.db`: main database (projects/endpoints/environments/mock rules/history)
- `master.key`: encryption key for environment variables (**do not delete**)
- `backups/`, `exports/`: backup and export directories

> Tip for full backups: copying the whole RustFox data directory to external storage migrates everything.

## 11. FAQ

| Problem | Fix |
| --- | --- |
| Nothing happens on double-click | Unpack portable Windows builds fully before running; on macOS right-click → Open the first time; on Linux install the WebKit libs (see 1.3) |
| "Not from an identified developer" | macOS first launch: right-click → Open; or allow in System Settings → Privacy & Security |
| SmartScreen warning | "More info → Run anyway" — normal for unsigned open-source apps |
| Environment variables look garbled/encrypted | `master.key` was lost; restore from a backup JSON (section 9) |
| Mock port busy | The port auto-increments; or stop whatever occupies it |
| OpenAPI import fails | OpenAPI 3.0 / Swagger 2.0 / Postman Collection v2.1 are supported (JSON or YAML); convert 3.1+ down first |
| Database corrupted? | No auto-repair, but the JSON in `backups/` can rebuild everything |
| Report a problem | The "Feedback" button in the top bar generates a local diagnostics report — attach it to a GitHub Issue |

## 12. Feedback & support

- "Feedback" in the top bar → generates a local environment/log summary → attach it to a GitHub Issue
- Data, deployment and advanced guides: `README.md` and `docs/DEPLOY.md` (Chinese)
