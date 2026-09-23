# Executable Goal：接续 GPTBridge 工具与权限补齐

在独立 Git 仓库 `/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal` 接续任务 `fcee046a-9ffc-453c-b2f6-0c4a7a6a7174`。本文件是执行真源；同名 HTML 仅由本文件生成。目标是收敛已存在的四个工具、权限预检和嵌套仓库 diff，实现必要修正，取得本轮新鲜回归及审查证据，精确提交并正常推送 `origin/main`。不得把其他会话的 Skill 写根、TaskDock、品牌/UI 改动混进本轮。

## 身份、模式与证据基线

| 字段 | 当前值 |
| --- | --- |
| `goalOwner` | 本次接续任务的 `/root`；只负责本任务 hunk、Goal 与最终报告 |
| `conversationChangeId` | `fcee046a-9ffc-453c-b2f6-0c4a7a6a7174`；沿用用户明确指定的同一任务身份 |
| `realpath(repo)` | `/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal` |
| `branch` / `baseSha` | `main` / `029ea777ad22fc5d8c3aaf4c4f6f8c685d35d905`，2026-09-23 本轮读取；提交前重查 |
| `origin` | `https://github.com/lixiangwelding/coding-tools-mcp-personal.git`，提交前重查；远端可能推进 |
| 模式 | `local`；`worktree_mode.py status --json` 显示 `enabled=false` |
| 仓库归属 | `companyProject=false`，个人 GitHub 仓库；`companyDlinter=NOT_APPLICABLE`，不向公司平台传仓库、文件或补丁 |
| CodeGraph | CLI 将嵌套仓库路由至父仓 `/Users/didi/my-project-java/codeVerifyRe0`；初查有 4 added / 4 modified pending，未把旧索引当当前事实 |
| GitNexus | 托管 1.6.9 可用；`list_tools_for_profile` 当前影响分析：`HIGH`、27 个影响点、10 个直接调用点、4 条受影响流程、3 个模块；索引是否覆盖最新脏改动须复核 |
| OpenGrok | 独立仓库未发现本地 OpenGrok 入口；本任务不创建或刷新索引 |
| 部署 | `deploymentCount=0`；不安装替换、重启 MCP/Actions、修改生产 Workspace 或刷新客户端缓存 |

事实来源：用户附件 `/Users/didi/.codex/attachments/00312982-3f9d-42be-aabd-dbb3643d4810/已粘贴的文本.txt`、仓内 `docs/gptbridge-tool-permission-audit-20260923-fcee046a.md` 和本轮直接读取的 Git/源码/工具回执。附件与旧交接都是历史快照；其历史测试、磁盘和运行版本不等于本轮验收。`codex无限` 的 `server_info`、`task_open(task_id=...)`、`task_status(task_id=...)` 本轮均返回 JSON-RPC 32600 `Session terminated`，因此尚不能确认远端最新 revision、未决 job 或 checkpoint；不猜根因、不假报已恢复。原生 Goal 当前检查为空；本仓其他 `global-skill-write-roots-c22eec0b` Goal 属于不同任务，不得接管。

## 本轮范围与所有权

本任务完整目标：实现状态、专项与完整测试、源码审查、精确 commit、正常 push 与远端 readback 均形成可复核证据。无法跨过的连接、磁盘、权限、并发所有权或真实测试门禁须留下本地成果和阻断证据，不能填 PASS。

`ownedPaths`：

- 独占新文件：`src-tauri/src/tools/toolbox.rs`、`src-tauri/src/tools/toolbox_tests.rs`、`docs/gptbridge-tool-permission-audit-20260923-fcee046a.md`、本 Goal 的 `.md`/`.html`、最终接续报告 `docs/gptbridge-tool-permission-continuation-20260923-fcee046a.html`（若已存在，先核对 SHA 与 owner）。
- 本任务修改前干净的文件：`src-tauri/src/tools/git.rs`；`src-tauri/src/tools/exec.rs` 的集成测试 worker 路径修正（写前 Git blob `5ad8238d32d4d1db9f7205f8e02e5ac37db56c6c`）。
- **仅拥有本任务 hunk，不拥有整文件**：`src-tauri/src/tools/mod.rs` 的 `toolbox` 接线，`src-tauri/src/tools/registry.rs` 的四工具目录/Schema 与 `git_diff.repo_path` 部分，`src-tauri/src/tools/dispatch.rs` 的预检/工具分发/default_cwd 部分，`src-tauri/tests/call_tool_contract.rs` 的本任务合同断言、权限预检测试调整和失败输出诊断。
- `src-tauri/tests/desktop_native.rs` 仅拥有**暂存树**中原版 33 → 本任务 37 工具及四工具存在性断言；共享工作树的 Skill 写根 39 工具断言归其他 owner，不能整文件暂存或覆盖。

上列共享文件还含另一个 Goal 的 `skill_write`/`apply_skill_patch` hunk。其他全部脏改、暂存、未跟踪文件均为 `foreign dirty diff`；尤其 `policy.rs`、`patch.rs`、`personal_patch.rs`、workspace 模型、Actions、Skill UI、TaskDock、品牌/README/package/Cargo 版本不归本任务。当前已有 39 个已跟踪文件变更，大量未跟踪文件，暂存区为空；这只是初查，写前与暂存前都须重查。任何路径被其他 owner 新改或无法把本任务 hunk 安全拆出时，停止该文件写入/提交，记录 `ACTIVE_GOAL_OWNERSHIP_CONFLICT` 或 `DIALOGUE_COMMIT_BLOCKED`。禁止 `git add -A`、`git add .`、整仓格式化、`reset --hard`、清理他人缓存或强推。

## 已有实现与必须核验的未知项

1. `check_command` 只调用本地执行策略及可执行文件预检，不执行、不发权限；结果只承诺 `allowed_locally/denied_locally`，平台授权、worker、任务存在性和最终命令成功均未知。八类 `request_permissions` 的 `exec_command` 请求应复用同一预检，硬边界失败不得返回 granted。
2. `tool_catalog_check` 对调用方提供的实际客户端目录比较工具名、输入字段和完整 Schema 指纹；只有调用方完整报告与真实客户端指纹一致才可称 `synchronized`，不能抄服务端指纹制造客户端验收。
3. `read_files` 限工作区内 1–20 个普通 UTF-8 文件；单文件默认 16 KiB/硬上限 64 KiB，总内容默认 128 KiB/硬上限 256 KiB，逐项错误、SHA、范围与 `next_index` 语义要核对。它不是原子快照，也不扩外部读取权限。
4. `stat_path` 只读元数据；文件系统只读属性不等于写授权，`write_authorized=null`。
5. `git_diff.repo_path` 选择工作区内的独立嵌套仓库；显式路径不被共享 `default_cwd` 重写，省略时旧行为兼容，截断保持 UTF-8 边界。
6. 历史专项 `14 passed`，全量 Cargo 在编译阶段因 `No space left on device` 失败。本轮直接 `df` 显示约 2.7 GiB 可用；仍须检查真实构建占用和新鲜回归。源码目录当前混合两项 Skill 写工具，测试计数 39；不把 39 写成运行端目录，也不为凑 37 删除别人的接线。
7. 本轮首次合同与全量回归发现现有 `nonzero_command_exit_keeps_transport_ok_but_sets_command_ok_false` 的持久 job 返回 `unknown/worker unavailable`。原因已定位：集成测试编译库时没有 `cfg(test)`，`personal_worker_path` 将测试可执行文件误当 worker。独立 `personal-runtime` worker 测试 14/14 通过；仅在可执行文件位于 `target/{profile}/deps` 时选择同级测试 worker 后，该失败用例单独回验通过。正式应用入口仍选择自身；必须继续做完整回归和独立提交树验证。
8. 修正 worker 后，当前**混合工作树**的合同测试 22/22 通过；全量回归中库测试 345/346，失败项为另一任务的 canonical-root 锁使原有同目录并发测试收到 `RESOURCE_BUSY`。本任务不得改其实现或把混合树失败归成本任务通过；必须以排除外国 hunk 的独立暂存候选树验证本任务提交。

## 权限与执行边界

- 用户本轮已授权本任务源码修改、回归、审查、精确 commit 和正常 push；未授权安装、运行替换、服务重启、生产/全局 Skill 写入或扩大 Home/平台权限。
- 仅使用本仓既有模式和最小必要工具。`codex无限` 连接恢复时按最新 Schema 重读 `server_info → task_open → task_status → git_status`；持续 `Session terminated` 时记录真实错误，继续独立的本地源码/测试工作，不能伪装远程 checkpoint 已更新。
- 所有外部 mutation 由本会话唯一 coordinator 执行；`DDT_ONE_MUTATION_COORDINATOR=/root`。本任务不创建子代理。只有提交后的普通 `git push origin main` 属于已授权外部写；`deploymentCount=0`。不触发 DDT 业务知识查询或 R2 备份。
- 共享源码每次写前核对文件 SHA、最新 diff 和其他 owner 变更；只补当前实现所需的最小 hunk。GitNexus 高风险符号写前先做 `impact` 并检查调用点，写后复核影响范围；索引过期时先刷新或明确降级到当前源码审查，不把旧图说成当前事实。

## 可执行步骤与依赖

1. **恢复身份与资源**：确认当前 `main`/HEAD/remote/暂存/foreign diff、目标文件 SHA，检查磁盘、运行进程与缓存占用。若需要清理，仅清点并删除本任务独占且可再生、无人使用的产物；不得删状态库、整个 `target` 或结束他人构建。读取 AGENTS、项目上下文、当前注册/分发/Actions/协议源码与交接。核对 active Goal/owner；冲突路径停止。
2. **影响分析与最小修正**：对 `list_tools_for_profile`、`call_tool`、`command_preflight`、`git_diff` 等拟改符号先做 GitNexus/当前源码影响分析；检查 MCP `core/advanced/read-only`、Actions/OpenAPI 边界，四工具输出与 8 类权限申请，不修改另一任务的 Skill 写接线。只在具体失败或风险证据下修正实现及有意义的测试。
3. **新鲜验证**：依序在目标仓库运行 `cargo test --manifest-path src-tauri/Cargo.toml --lib toolbox -- --nocapture`，`cargo test --manifest-path src-tauri/Cargo.toml --test call_tool_contract`，`cargo test --manifest-path src-tauri/Cargo.toml`，`git diff --check`。验证预检无执行/假授权、客户端缺字段、批读边界/软链接/UTF-8/截断、`repo_path` 与兼容行为。若 UI/发布制品不在本任务范围，不扩做前端构建。记录命令、时间、退出码、测试数、树/制品 SHA；历史 PASS 不替代当前结果。
4. **提交树审查**：运行当前可用 `detect_changes`/`code_review` 或等效源码审查；核对 Actions 只读映射、共享接线与契约。用精确 hunk 暂存本任务文件，检查 `git diff --cached --check`、staged 文件/hunk、敏感信息与生成产物排除。对精确暂存树做可行的独立验证；若混合工作树测试无法证明独立提交树，明确 `DIALOGUE_COMMIT_BLOCKED`，不提交不完整树。
5. **交付**：重查 `origin/main`，并发推进时先核对，不 rebase/reset 覆盖。仅本任务树创建普通 commit，非强制推送 `origin main`，用 `git ls-remote`/`git fetch` 等只读回读证明远端包含该 SHA。记录 commit、文件与 hunk、push 输出；不能以本地 ahead/behind 代替远端验收。
6. **收口**：更新同一任务交接和最终接续 HTML；若 `codex无限` 恢复，用最新 revision 的 `task_checkpoint` 只写本阶段 delta。分别报告源码、专项/完整回归、审查、commit、push、安装/运行版本、真实客户端工具目录及未完成项。`codex无限` 未恢复时 `checkpoint=BLOCKED_SESSION_TERMINATED`；不把本地报告冒充远端 checkpoint。

## 完成、停止与回滚

成功需要：当前目标源码无未处理高优先级缺陷；本轮最终候选树的专项、合同和全量测试通过；独立提交树完整；普通 commit/push 与远端 readback 均有回执。运行版本和客户端目录若未验证，分别写 `NOT_VERIFIED`，不能阻止已授权的源码交付，也不能声称工具在桌面可用。

停止依赖步骤的条件：无法确认目标仓库/owner、共享 hunk 不能安全切分、磁盘/状态库导致测试无新鲜证据、未解决高优先级审查问题、远端并发推进或 push 权限拒绝。继续不依赖的本地准备与交接，保存失败命令和退出信息，不无限重试。回滚只针对本任务新 commit 使用普通反向提交或精确本任务 hunk 恢复；不得整仓重置、清理 foreign diff 或强制推送。若 push 已成功，先核对远端再制定同范围可审查的 revert。

最终报告字段：`goalStatus`、`repoRealpath`、`branch`、`baseSha`、`conversationChangeId`、`ownedPaths`、`foreignDiffExcluded`、`sourceImplementationStatus`、`freshTestEvidence`、`reviewEvidence`、`commitSha`、`pushEvidence`、`runningVersionStatus`、`clientCatalogStatus`、`checkpointStatus`、`deploymentCount=0`、`remainingIssues`、`rollbackPoint`、Goal `.md`/`.html` 绝对路径。

精确启动入口：

```text
/goal /Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal/docs/goals/gptbridge-tool-permission-continuation-fcee046a-goal.md
```
