# AI Agent 集成指南

RustFox 内置 **Agent 控制面**：应用启动时自动拉起一个本机 HTTP 服务（仅 `127.0.0.1`），
AI Agent 可以直接把 cURL 命令保存为 RustFox 接口，也可以查询项目与接口列表。

两种接入方式：

| 方式 | 适用 | 前置 |
| --- | --- | --- |
| MCP Server（推荐） | Claude Code、Cursor 等支持 MCP 的客户端 | 额外获取 `rustfox-mcp` 二进制 |
| 直接 HTTP | 任意能发 HTTP 请求 / 执行命令的 Agent | 无 |

---

## 1. 工作原理

```
AI Agent ──(stdio MCP)──→ rustfox-mcp ──┐
                                        ├──→ Agent 控制面 (127.0.0.1:4110~4129)
AI Agent ──(HTTP + Bearer token)────────┘         │
                                             SQLite 落库 → UI 实时刷新
```

- 控制面随 **RustFox 桌面应用启动自动拉起**，无需手动开启；
- 端口从 `4110` 起自动探测（避开 Mock 的 4010 段）；
- 所有请求需携带令牌：`Authorization: Bearer <token>` 或 `X-Agent-Token`。

## 2. 找到你的 Token

Token 是控制面的访问凭证，首次启动应用时自动生成：

- 文件位置：`{数据目录}/agent-token`（权限 0600）
- 数据目录：
  - Windows：`%APPDATA%\RustFox\agent-token`
  - Linux：`~/.local/share/RustFox/agent-token`
- 应用内也可确认：顶栏「设置」旁无入口时，可在 DevTools Console 执行
  `await __TAURI_INTERNALS__.invoke('plugin:fox|agent_status')` 查看 `tokenPath`。

> 开发构建（tauri dev）使用 `RustFox-dev` 目录，与正式版隔离。

## 3. 方式一：MCP Server

### 3.1 获取 rustfox-mcp

**v0.0.10 起，安装包已内置 `rustfox-mcp`**，按平台取对应路径即可：

| 平台 / 安装方式 | 路径 |
| --- | --- |
| macOS（/Applications 安装） | `/Applications/RustFox.app/Contents/MacOS/rustfox-mcp` |
| Windows（NSIS 默认目录） | `C:\Program Files\RustFox\rustfox-mcp.exe` |
| Linux（.deb） | `/usr/bin/rustfox-mcp`（已在 PATH，配置直接写 `rustfox-mcp`） |
| Linux（.AppImage） | 挂载镜像内的 `rustfox-mcp` |
| 开发模式（tauri dev） | 仓库内 `frontend/src-tauri/binaries/rustfox-mcp-<三元组>` |

<details>
<summary>旧版本（&lt; v0.0.10）或从源码构建</summary>

```bash
git clone https://github.com/weihubeats/RustFox.git
cd RustFox && cargo build --release -p fox-mcp
# 产物：target/release/rustfox-mcp
sudo cp target/release/rustfox-mcp /usr/local/bin/   # 可选：放进 PATH
```
</details>

### 3.2 配置客户端

**Claude Code** — 项目根目录 `.mcp.json`（v0.0.10+ 按上表替换为安装包内路径）：

```json
{
  "mcpServers": {
    "rustfox": { "command": "rustfox-mcp" }
  }
}
```

若二进制不在 PATH，用绝对路径：

```json
{ "command": "/Applications/RustFox.app/Contents/MacOS/rustfox-mcp" }
```

**Cursor** — Settings → MCP → Add Server，Command 填 `rustfox-mcp`。

配置后重启客户端 / 重载会话，工具列表出现 4 个工具即成功：

| 工具 | 说明 |
| --- | --- |
| `save_curl` | 解析 cURL 并保存为接口（返回 endpointId） |
| `list_projects` | 项目列表 |
| `list_endpoints` | 项目下的接口列表（需 projectId） |
| `agent_info` | 控制面地址与令牌路径 |

之后对话里说「把这个 curl 存到 RustFox」即可；导入成功桌面端侧栏自动刷新并弹提示。

### 3.3 save_curl 参数说明

| 参数 | 必填 | 说明 |
| --- | --- | --- |
| command | ✓ | 完整 cURL 命令字符串 |
| name | | 接口名；缺省从 URL 路径末段推导 |
| projectId | | 目标项目；缺省时唯一项目自动选中，零项目自动创建「Agent 导入」，多项目报错并列出候选 |
| folderId | | 归属文件夹 |

导入行为：

- URL 拆解为 base_url + 路径 + query 参数（与手动 cURL 导入一致），接口状态为「设计中」草稿；
- 项目变量 `base_url` 缺失时自动写入本次 URL 的 origin；
- **已有不同 base_url 时不覆盖**，响应带 `warning` 字段，Agent 会转达。

## 4. 方式二：直接 HTTP

适合自研脚本或暂不支持 MCP 的 Agent。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/agent/curl` | `{command, projectId?, name?, folderId?}` → `{endpointId, ...}` |
| GET | `/agent/projects` | 项目列表 |
| GET | `/agent/endpoints/:projectId` | 接口列表 |
| GET | `/agent/health` | 存活探针 |

示例：

```bash
TOKEN=$(cat "$HOME/Library/Application Support/RustFox/agent-token")
curl -s http://127.0.0.1:4110/agent/curl \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"command":"curl -X POST -H \"Content-Type: application/json\" -d \"{\\\"a\\\":1}\" https://api.example.com/orders"}'
```

字段同时接受 camelCase 与 snake_case（`projectId` / `project_id`）。
错误体统一 `{code, message}`：`VALIDATION`(400) / `NOT_FOUND`(404) / `UNAUTHORIZED`(401)。

## 5. 安全设计

- 只绑定 `127.0.0.1`，外网不可达；
- 随机 UUID 令牌，文件权限 0600；
- 写操作仅限「导入接口」一种，不可删除/修改已有数据；
- 不读取密钥环中的加密环境变量明文。

## 6. 排障

| 现象 | 处理 |
| --- | --- |
| `spawn rustfox-mcp ENOENT` | 二进制不在 PATH：v0.0.10+ 改用安装包内绝对路径（见 §3.1），或按旧版方式构建后放入 PATH |
| `rustfox-mcp` 报「未发现运行中的控制面」 | 先启动 RustFox 桌面应用；确认端口 4110~4129 未被防火墙拦本机回环 |
| 401 UNAUTHORIZED | token 文件与应用不一致（如混用了 dev/正式版目录）；删掉 `agent-token` 后重启应用重新配 |
| 多项目时报 VALIDATION | 让 Agent 先调 `list_projects`，带上 projectId 重试 |
| MCP 工具列表没出现 | 检查 `.mcp.json` 语法与 rustfox-mcp 可执行权限（`chmod +x`）；查看客户端 MCP 日志 |
| 导入后 UI 没刷新 | 仅当桌面端正打开同一项目才实时刷新；切项目或重进工作区可见 |
