# GPTBridge

### Bring the conversation to your workspace.

**An AI task workbench connecting ChatGPT to local projects, tools and skills.** Keep progress, resumable context and execution records in one place, without turning the home screen into a configuration manual.

[English](README.en.md) · [中文](README.md) · [Usage and compatibility](PERSONAL.md) · [Brand and version boundaries](docs/gptbridge/BRANDING.md)

## Five focused areas

| Area | Purpose |
| --- | --- |
| Workbench | Search and filter real task records, inspect checkpoints, job status and bounded logs |
| Projects | Select a local directory, switch context, manage connections and advanced settings |
| MCP connections | Open the selected project's connection configuration directly, including ports, authentication, tunnels and explicitly authorized Skill write roots |
| Tools and skills | Discover local skills, inspect their sources and tool contracts, control project-level automatic matching |
| Settings | Keep common preferences visible and advanced configuration out of the daily workflow |

## Start once, continue with context

![GPTBridge 0.4.1 workbench: browser preview of the actual frontend build](docs/gptbridge/screenshots/browser-preview-0.4.1/workbench-desktop.png)

**Historical environment: a browser preview of the actual 0.4.1 frontend build, captured before the new top-level MCP connections entry, without Tauri IPC or injected demo tasks.** It records the disconnected state at capture time, not the final interface or native-backend acceptance for this continuation. See [all eight historical desktop and narrow-viewport screenshots](docs/gptbridge/SCREENSHOTS.md) and the [validation record](docs/gptbridge/VALIDATION.md).

Select a project, configure the client connection, describe a goal, inspect progress and continue in your connected AI client. **Saving a goal does not start an agent.** Tasks without running jobs remain ready for client handoff. A reachable port is not proof of a client handshake.

## Version 0.4.1

The public product name is now **GPTBridge**, following the TaskDock workbench redesign. The application identifier, configuration paths, task database, IPC command names and MCP endpoints retain compatibility. Svelte 5 / SvelteKit and Tauri 2 / Rust remain the implementation stack.

This is a source update, not a claim that a signed installer, native desktop acceptance or a live service replacement has shipped. Screenshots must identify their environment; browser previews are not native-backend acceptance and do not substitute mock records for missing task data.

```sh
npm ci
npm run check
npm run build
# Native desktop development: npm run desktop
```

GPTBridge is an independent project maintained by `lixiangwelding`, derived from the Rust/Tauri `mybolide/coding-tools-mcp` implementation. Upstream attribution and licenses are retained. It is not an official OpenAI product and does not imply endorsement.

<details>
<summary>Historical reference: earlier personal and upstream versions</summary>

The following screenshots, download links, menu locations and release claims describe historical versions, not GPTBridge 0.4.1.

0.3.3 fixes the `/mcp` event-stream GET response: requests accepting `text/event-stream` now receive `405`, while ordinary health checks can still read the JSON version payload.

Added in 0.3.2: [local skill discovery and source references](docs/local-skills.md). Native Rust tools read existing skill directories and resolve submitted `$name` text. This does not register a native ChatGPT composer picker or run skill scripts.

This is the independent `lixiangwelding` personal fork. See [PERSONAL.md](PERSONAL.md) for the new task workflow, cooperative same-directory writes, durable execution, isolated configuration, and reproducible tests. No worktree or pasted initialization template is required.

Version 0.3.1 adds an opt-in, authenticated multi-repository gateway on one MCP endpoint and one FRP route. See [shared-gateway.md](docs/shared-gateway.md). It also repairs native configuration loading, atomic task creation, and sequential same-file patches. The running legacy service is not modified.

Build/import/test do not start or stop the legacy application. The upstream desktop documentation below is retained for reference; its downloads and legacy history workflow are not this fork's release artifacts.

---

<p align="center">
  <img src="src-tauri/icons/128x128.png" width="96" alt="Coding Tools MCP icon">
</p>

<h1 align="center">Coding Tools MCP</h1>

<p align="center">
  Turn a local project into a persistent AI development workspace that carries context across conversations.
</p>

<p align="center">
  <a href="https://github.com/mybolide/coding-tools-mcp/releases/latest"><img src="https://img.shields.io/github/v/release/mybolide/coding-tools-mcp?label=Release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Windows-x64-0078D4?logo=windows" alt="Windows x64">
  <img src="https://img.shields.io/badge/macOS-Apple%20Silicon-000000?logo=apple" alt="macOS Apple Silicon">
  <a href="https://www.apache.org/licenses/LICENSE-2.0"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="Apache-2.0"></a>
</p>

<p align="center">
  <a href="README.md">中文</a> · <a href="README.en.md">English</a> · <a href="https://github.com/mybolide/coding-tools-mcp/releases/latest">Download latest</a>
</p>

Coding Tools MCP is a Rust + Tauri 2 desktop application. Select a project directory and start the service; an AI agent can then read files, edit code, run commands and tests, inspect Git, and preserve development progress inside the project through MCP. It behaves like an AI opening an IDE workspace that remembers where the last conversation stopped.

![Coding Tools MCP workspace overview](docs/images/workspace-overview.png)

*One desktop app manages workspaces, MCP services, connection details, and the session-recovery prompt.*

## Understand the workflow in 30 seconds

```text
Install the desktop app
  → add a project directory
  → start MCP and a public tunnel
  → copy the Public MCP URL
  → enable ChatGPT developer mode
  → create an MCP plugin and paste the URL
  → authorize it and start developing in a new conversation
```

For a first connection, remember only this: **the desktop app turns the project into an MCP workspace, and ChatGPT connects to it through the public `/mcp` URL.**

- [See the complete desktop setup](#get-started-in-five-minutes)
- [Go directly to the ChatGPT plugin setup](#mcp-connector)

## Get started in five minutes

### 1. Install the desktop client

Open [Releases](https://github.com/mybolide/coding-tools-mcp/releases/latest) and download the package for your platform:

| Platform | Package |
| --- | --- |
| Windows 10/11 x64 | `Coding.Tools.MCP_*_x64-setup.exe` |
| macOS Apple Silicon | `Coding Tools MCP_*_aarch64.dmg` |

The macOS build is currently unsigned. If macOS blocks the first launch, allow it from System Settings → Privacy & Security.

### 2. Add a project workspace

1. Click **Add workspace** in the sidebar.
2. Select the project root directory.
3. Configure the workspace name, MCP port, and authentication mode.
4. Save it. The workspace remains available in the sidebar across conversations and restarts.

### 3. Configure a public tunnel

When the AI client is not running on the same machine, expose MCP through HTTPS:

- Install or detect `frpc` / `cloudflared` from **Software management**.
- Save the server, port, and token under **FRP settings**, or select Cloudflare in the workspace.
- Give each workspace a distinct subdomain. The app manages the FRP process and aggregates multiple proxy routes.

![FRP configuration](docs/images/frp-configuration.png)

*FRP server profiles are stored centrally; each workspace only selects a profile and supplies its own subdomain.*

If you do not have an FRPS server yet, follow this [FRPS server installation guide (Chinese, WeChat)](https://mp.weixin.qq.com/s/kmpQhHsvmHlaLfj4rw3A0Q). After deployment, enter the server address, port, and token under **FRP settings** in the desktop client.

### 4. Start MCP

Open the workspace and click **Start** in the MCP panel. The desktop client shows:

- a local MCP URL such as `http://127.0.0.1:28766/mcp`;
- the public HTTPS MCP URL;
- authentication details for ChatGPT;
- live logs and health-check results.

![Local, public, and ChatGPT MCP connection details](docs/images/workspace-connection.png)

The desktop app can verify the local and public endpoints, OAuth metadata, and the MCP protected-resource document:

![MCP health-check results](docs/images/health-check.png)

*Each connectivity and authentication check reports its result separately.*

When a connection fails, inspect recent MCP requests without leaving the desktop app:

![MCP runtime logs](docs/images/runtime-logs.png)

*The log quickly confirms whether tool discovery, history bootstrap, and checkpoint calls reached the server.*

### 5. Connect an AI client

Use the public MCP URL shown by the app. With OAuth enabled, the client follows the server metadata into the authorization flow; authorization codes, Client IDs, and secrets can be generated and managed from the desktop client. This release uses preconfigured OAuth clients, so select static/manual OAuth credentials when creating a ChatGPT plugin; CIMD is not required.

For a first connection, ask the agent to initialize history before inspecting the workspace:

```text
history_session_bootstrap
server_info
get_default_cwd
git_status
check_exec_environment
```

This gives the agent explicit project and capability state instead of guessing from the current chat window.

## Two ways to connect ChatGPT

| Mode | Best for | Use this endpoint |
| --- | --- | --- |
| MCP Connector | Direct access to files, commands, and Git | the workspace's public `/mcp` URL |
| GPT Actions | Importing OpenAPI tools into a custom GPT | the Actions panel's `/openapi.json` URL |

### MCP Connector

Before configuring ChatGPT, make sure that:

1. The workspace MCP service and public tunnel are both running.
2. The public MCP endpoint passes the desktop health check. If OAuth is enabled, also verify the protected-resource document and authorization metadata.
3. You have copied the **Public MCP URL** from the desktop **GPT configuration** card. For OAuth, also have the OAuth Client ID, OAuth Client Secret, and authorization password ready.

> ChatGPT must use the public HTTPS `/mcp` URL. A local address such as `http://127.0.0.1:28766/mcp` is not reachable from ChatGPT. Menu names may vary slightly by ChatGPT version and language.

#### 1. Enable ChatGPT developer mode

Open ChatGPT settings, go to **Account security and sign-in**, and enable **Developer mode**. This allows unverified MCP connectors to be added.

![Enable developer mode in ChatGPT](docs/images/gpt-config-1.png)

*Developer mode grants powerful access. Only connect MCP servers that you operate or explicitly trust.*

#### 2. Create the MCP plugin

Open **Plugins** from the ChatGPT sidebar, click the `+` button, select the MCP beta option, and enter:

| ChatGPT field | Value |
| --- | --- |
| Name | A recognizable name such as `Coding Tools MCP` |
| Description | A short description of the connected project or purpose |
| Connection | The public MCP URL from the desktop **GPT configuration** card; it should end in `/mcp` |
| Authentication | The same mode configured in the desktop app; the screenshot uses OAuth |

![Create an MCP plugin and enter its connection details](docs/images/gpt-config-2-detail.png)

For OAuth, open the advanced OAuth settings, select static/manual OAuth credentials, and enter the Client ID and Client Secret shown by the desktop app. CIMD is not required. When ChatGPT opens the authorization page, enter the authorization password from the desktop **GPT configuration** card.

> Client Secrets, authorization passwords, and Bearer tokens are sensitive. Never paste them into chats, issues, or public screenshots. If the desktop app uses Bearer or no authentication, select the matching option currently offered by ChatGPT.

#### 3. Verify the connection

Start a new conversation with the plugin enabled and ask:

```text
Use Coding Tools MCP to call server_info, get_default_cwd, and git_status.
Tell me which workspace is connected, its default directory, and its Git status.
```

If ChatGPT returns information from the current project, the desktop app, public tunnel, authentication, ChatGPT, and MCP tool chain are connected end to end. Before real development, call `history_session_bootstrap` to initialize or restore project history.

If ChatGPT still shows an old tool list, disconnect and reconnect the plugin or verify again in a new conversation.

#### Troubleshooting

| Symptom | Check first |
| --- | --- |
| ChatGPT cannot connect | Confirm that the URL is the public HTTPS `/mcp` endpoint rather than `127.0.0.1`, and that the public MCP health check passes |
| OAuth authorization fails | Confirm that the Client ID, Client Secret, and authorization password come from the same workspace, and check the OAuth metadata results |
| New tools are missing | Disconnect and reconnect the plugin, then start a new conversation |
| A tool call fails | Open **Logs** and **Health checks** in the desktop app and confirm that the request reached the MCP service |

### Expose configured local MCP servers

In addition to the app's built-in development tools, a workspace can proxy MCP servers that an administrator explicitly configures. It supports **stdio** for local processes and **Streamable HTTP** for local or reachable services. ChatGPT and cloud Codex continue to connect only to this app's public `/mcp` endpoint; the desktop app forwards calls to the saved upstreams.

In the workspace's **MCP → Configuration → Local MCP** area, click **+ Add**:

1. Choose **Quick create**, then select the `stdio` or `Streamable HTTP` transport.
2. Or choose **Import from JSON** and paste a single configuration, an array of configurations, or a conventional `mcpServers` object. Imports create drafts only; review and save them before use.
3. For `stdio`, provide the command, arguments, and environment variables. For `Streamable HTTP`, provide the endpoint URL and optional request headers.
4. Enable the local MCP and click **Fetch tools**.
5. All discovered tools are exposed by default; turn off any high-privilege tools that should stay private.
6. Save. The desktop app reloads a running MCP service, or applies the configuration the next time it starts.

Example Streamable HTTP import:

```json
{
  "mcpServers": {
    "local-service": {
      "type": "streamableHttp",
      "url": "http://127.0.0.1:3000/mcp",
      "headers": {
        "Authorization": "Bearer your-token"
      },
      "tool_prefix": "local-service"
    }
  }
}
```

Remote clients see tools with the workspace-configured prefix, for example `local__tool_name`. Existing manual allowlists retain their current exposure until they are saved through the per-tool toggles in the new interface.

> Security boundary: environment variables are injected only into local stdio processes, and HTTP request headers are sent only to their saved endpoints. Neither appears in the `/mcp` tool directory, tool responses, or ordinary request logs. Automatically exposing tools grants remote clients every capability offered by that local service; turn off unwanted tools before saving, and do not configure untrusted MCP servers or environment variables containing sensitive credentials.

### GPT Actions

1. Start the workspace Actions service.
2. Copy the OpenAPI URL from the Actions panel.
3. Import the URL in the GPT editor's Actions page.
4. Select None, API Key, or OAuth to match the desktop configuration.

MCP and Actions can run together for the same workspace, with separate ports and subdomains when needed.

## Why use it

- **Built for real development**: files, commands, Git, tests, and retained processes live in one Workspace.
- **Cross-conversation continuity**: a new conversation can recover the complete history summary and the latest detailed handoff.
- **Auditable progress**: structured checkpoints preserve decisions, changed files, test results, remaining issues, and next steps inside the project.
- **Multiple workspaces**: one desktop client stores multiple projects and manages their MCP, Actions, and public endpoints.
- **Direct ChatGPT connectivity**: Streamable HTTP, OAuth, Bearer tokens, OpenAPI, FRP, and Cloudflare are built in.
- **A focused default tool surface**: stable core tools are available by default; advanced Harness capabilities are opt-in.

## Let the project remember every conversation

Chat transcripts are useful for rereading a discussion, but they are a poor long-term development handoff. Coding Tools MCP stores progress in `docs/history-session/` under the current project, so context follows the repository instead of staying trapped in one chat window.

![ChatGPT new-conversation startup prompt](docs/images/history-session-prompt.png)

*Paste the full prompt into a new conversation to initialize or restore history, then save a checkpoint after each completed task.*

Five tools work together:

| Tool | Purpose |
| --- | --- |
| `history_session_bootstrap` | Initialize or restore a project session; preserve verbatim `initial_user_input` and return a stable `session_key`, `current_path`, and bounded current state instead of all history |
| `history_session_checkpoint` | Append structured progress and verbatim `raw_user_input` to the stable target returned by bootstrap; reject mismatched targets instead of writing to another history file |
| `history_session_validate` | Validate numbering, history files, and session mappings; rebuild derived indexes when needed without deleting existing history |
| `history_session_search` | Search lossless Markdown archives by deterministic keywords and return a bounded page of locations and snippets |
| `history_session_read` | Read one original Markdown archive losslessly in UTF-8-safe pages by number or a search result path; pages default to `32 KiB`, are capped at `64 KiB`, and continue with `next_cursor` |

History uses readable Markdown that can be backed up or committed with the project. `memory/state.json` is a bounded current-state projection, while `memory/manifest.json` stores only archive locations, hashes, and keywords; Markdown remains the lossless source of truth. ChatGPT must pass verbatim first-turn and per-turn text as `initial_user_input` and `raw_user_input`, because the server cannot inspect remote chat text that was not provided as a tool argument. Checkpoints are idempotent, changed content for the same `turn_id` is retained as a revision with supersession evidence, and progress should only be reported as saved after the tool returns `ok=true` with the same session target.

> History persistence is performed when the AI calls the MCP tools; the desktop app does not record chat content in the background. If the client does not invoke a tool, the server cannot infer that a new conversation or task has happened.

## What an agent can do

The default `core` profile provides a stable, composable development tool set:

| Category | Main tools |
| --- | --- |
| File reading | `read_file`, `list_dir`, `list_files`, `search_text`, `grep_text`, `view_image` |
| File modification | `apply_patch` |
| Command execution | `exec_command`, `write_stdin`, `read_output`, `kill_session` |
| Git | `git_status`, `git_diff`, `git_log`, `git_show`, `git_blame` |
| Environment | `server_info`, `check_exec_environment`, `get_default_cwd`, `set_default_cwd` |
| History sessions | `history_session_bootstrap`, `history_session_checkpoint`, `history_session_validate`, `history_session_search`, `history_session_read` |

A typical development loop is:

```text
Open Workspace
  → understand project and Git state
  → search and read code
  → apply a transactional patch
  → run commands and tests
  → inspect the diff and commit
```

The advanced profile retains project-state and operation-history Harness capabilities, but normal edits and command execution do not require a Task.

## Permission and recovery model

The project uses a Workspace-first permission model:

- Normal files inside the Workspace can be read, created, modified, deleted, and executed.
- Outside the Workspace, `read_file`, `list_dir`, `list_files`, `search_text`, and `view_image` provide read-only access.
- Writes, deletes, and command execution outside the Workspace are blocked.
- `.git` and `.github` cannot be damaged through ordinary file tools, Patch, or interpreter commands.
- Patch performs preflight validation and operation-local recovery; long-term recovery uses Git instead of full Workspace snapshots.

> Windows child-process execution currently uses a `policy_only` boundary. The honest runtime value is `sandbox_enforced: false`; static command policy is not a complete OS filesystem sandbox.

## Local development

Requirements: Node.js 20+, Rust stable, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
npm install
npm run desktop
```

Useful verification commands:

```bash
npm run check
npm run build
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

On Windows, you can also run `dev-desktop.cmd`. Do not use `npm run dev` alone to validate the desktop application; it starts Vite without the Tauri shell.

## Project layout

| Path | Purpose |
| --- | --- |
| `src-tauri/src/tools/` | Shared file, Patch, Exec, and Git tool kernel |
| `src-tauri/src/mcp/` | MCP Streamable HTTP server |
| `src-tauri/src/actions/` | ChatGPT Actions OpenAPI gateway |
| `src-tauri/src/tunnel/` | FRP / Cloudflare tunnel and process management |
| `src/` | SvelteKit desktop UI |
| `old/` | Python reference implementation and compatibility baseline |

## License

[Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0)

</details>
