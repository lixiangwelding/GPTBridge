# GPTBridge 工具与权限补齐：实现及接续交接

任务：`fcee046a-9ffc-453c-b2f6-0c4a7a6a7174`。
日期：2026-09-23。本文是证据快照，不得覆盖后续改动。
工作区：`/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal`。

## 上轮结论（历史快照；接续结果见文末）

本轮已实际修改源码，并通过全部 14 项新增专项单元测试。完整 Cargo 回归在编译阶段因磁盘空间不足失败，此后持久命令执行器出现 STATE_DB 磁盘 I/O 错误，提交前图谱审查及 Git 命令无法启动。**尚未 commit、push、安装替换或刷新客户端工具目录。**

最后核验：独立仓库 main，HEAD `029ea777ad22fc5d8c3aaf4c4f6f8c685d35d905`，origin/main 相同；工作区已有其他会话的品牌改名、全局 Skill 写入根及 UI 改动，暂存区未见条目。本轮没有整仓暂存、回退、强推或重启运行服务。

本地 package.json 显示 `gptbridge` / 0.4.1；运行中的连接仍为 Coding Tools MCP Personal 0.3.8 / core 33 工具。本地组合源码包含全局 Skill 写入的 2 个协作工具，加上本轮 4 个新工具后测试目录为 39。**源码数量不代表当前客户端已经可调用；独立提交本轮而不含 Skill 写入时应为 37。**

## 本轮新增四个只读工具

| 工具 | 功能与边界 |
|---|---|
| `check_command` | 使用现有执行策略和可执行文件解析器检查 exec_command 参数；不执行命令、不发放权限。返回 allowed_locally / denied_locally，平台授权不可观察。任务存在性、worker、资源锁、退出状态不属于预检保证。 |
| `tool_catalog_check` | 对比客户端提供的工具名、输入字段及目录 SHA；报告缺失工具、漏字段、重复项和未知项。只有完整工具、字段和完整 schema 指纹均匹配才报告 synchronized，不能把仅名称一致当成同步成功。 |
| `read_files` | 一次读取 1–20 个工作区内普通 UTF-8 文件，保留每个文件的错误及原读取工具提供的 SHA 信息；单文件内容上限 64 KiB、总内容上限 256 KiB，支持 next_index。不是多文件原子快照，不扩展外部读取权限。 |
| `stat_path` | 获取工作区内现有文件/目录的类型、大小、修改时间和文件系统只读属性；不读取正文、不修改权限，write_authorized 明确未知。 |

## 修复

1. `request_permissions` 的 exec_command 预检不再只检查 privileged_executable；8 种权限申请均先复用同一个本地预检，防止工作目录/执行边界仍拒绝却返回 granted。
2. `git_diff` 增加向后兼容的 repo_path，支持主工作区内的独立嵌套 Git 仓库；显式仓库选择不会再被共享 default_cwd 重写。旧参数省略时保持原工作区行为。
3. Git diff 按 UTF-8 字符边界截断，避免中文截断产生替换字符及超过内容字节预算。

未把整个 home 设成可写，也未关闭工作区隔离、并发 SHA 校验或破坏性操作确认。已有的全局 Skill 写入根、虚拟环境与 lsof 协作变更不算本轮新增交付，需其负责人继续验证与发布。

## 变更归属

新文件（本轮独占）：
`src-tauri/src/tools/toolbox.rs`、`src-tauri/src/tools/toolbox_tests.rs`、本交接文件。

原先无改动而本轮修改：`src-tauri/src/tools/git.rs`。

与协作会话共享的修改文件：`src-tauri/src/tools/mod.rs`、`src-tauri/src/tools/registry.rs`、`src-tauri/src/tools/dispatch.rs`、`src-tauri/tests/call_tool_contract.rs`。
这些文件必须按 diff/hunk 精确区分；不得直接 git add -A，也不得假定当前全部差异属于本轮。

## 验证证据

- `cargo test --manifest-path src-tauri/Cargo.toml --lib toolbox -- --nocapture`：exit 0，14 passed / 0 failed。Job `13639807-37d7-42cc-8a5d-5ffabe507d59`；源码编译为 0.4.1。测试覆盖目录契约、参数漂移、批量读取预算与错误、SHA、越界/软链接、元数据不冒充授权、预检不执行、8 种权限硬边界、任务参数及嵌套 Git diff。
- `cargo test --manifest-path src-tauri/Cargo.toml`：exit 101，编译报 No space left on device (os error 28)，并非已运行的断言失败。Job `9a42387b-b386-4343-bc05-d43b4a49e3d8`。
- 磁盘检查：total 494384795648 / used 494260727808 / free 124067840 字节，为当时快照。Job `939b789d-309c-4003-bdba-645c0f870d67`。
- 尝试直接运行已编译的全量库测试、执行 code_review、检查 Git 暂存区均在命令启动前返回 STATE_DB（disk I/O error / unable to open database file），没有创建可继续轮询的新 job。
- GitNexus 注册表影响分析成功：HIGH，27 个影响点、10 个直接调用点、3 个模块。Job `94916bab-6b61-4f86-83c1-13adec1094f4`。git_diff / call_tool 的影响分析命令亦 exit 0；提交前 detect_changes 尚未完成，不可冒称通过。
- GitHub 连接只读取远程 main，确认与基线一致；未创建远程提交或更新分支，避免本地磁盘损坏时让并发会话的共同工作副本与远程脱节。

## 恢复顺序

先恢复本机磁盘可用空间及持久命令状态库，不强删他人任务缓存或运行进程。恢复后重新读取任务、HEAD、当前文件 SHA 和其他会话的最新提交；不要按此快照回滚源码。

执行完整后端/协议测试及提交前影响审查，复核只读工具注解、两种目录数量与实际提交内容一致。随后精确暂存本轮差异，检查 staged diff，正常 commit 并 push origin main。若其他会话已提交全局 Skill 写入或品牌升级，以最新 HEAD 为准重新收敛，不强推。

安装/服务切换应另外验证运行版本与客户端 tools/list；不停止其他任务、不把源码构建或 server_info 指纹当成客户端刷新成功。

## 2026-09-23 本轮接续证据

本轮以独立仓库 `main` 的 `029ea777ad22fc5d8c3aaf4c4f6f8c685d35d905` 为基线。`origin/main` 在提交前只读回查仍是同一 SHA。先按 hunk 精确暂存本任务的四个工具、预检、嵌套 Git diff、worker 路径修正、契约测试及 Goal；共享文件中的 Skill 写入根等其他会话改动保留在未暂存区。隔离暂存树的补丁 SHA-256 为 `02cc3272bc93ddc5e31bb7516f594c6f6b774afe7e381456c3b5d3408bb210a4`；验证时隔离树的代码版本为 0.4.0、core 37 个工具，不能把混合工作树的 0.4.1/core 39 写成该提交的结果。

隔离树来自 `git archive HEAD` 加已暂存补丁；逐个暂存路径与 Git index 比对一致。使用 `--locked --offline` 和复用的 Cargo target 验证：

- `cargo test --manifest-path <隔离树>/src-tauri/Cargo.toml --locked --offline --lib toolbox -- --nocapture`：exit 0，14/14。
- 同一路径 `--test call_tool_contract`：exit 0，22/22。
- 同一路径完整 `cargo test --locked --offline`：exit 0；334 个库测试、8 个 main 测试及全部集成测试通过，未见失败项。
- 混合工作树的完整测试曾有 1 项属于其他会话 `personal_patch.rs` 写根锁的 `RESOURCE_BUSY` 断言失败；隔离树完整回归通过，故未将该外来改动混入本轮提交。

新增 `exec.rs` 修正测试可执行文件位于 `target/{profile}/deps` 时的 worker 查找；此前隔离树的契约测试会把测试进程当成 worker，返回 `unknown/worker unavailable`。修正后契约及完整回归通过。生产应用可执行文件不在 `deps` 时仍走原有路径。

GitNexus `detect-changes --scope staged` 返回 11 个暂存文件、16 个符号、8 条受影响流程，风险级别 high。刷新 GitNexus 索引时因 `ENOSPC` 中止，因此该影响检测使用已有索引，不能声称覆盖最新脏改动。`mcp-probe-kit code_review` 已提供暂存 diff；人工逐项审阅工具注册、MCP/Actions 共用分发、权限硬边界、读文件范围与预算、目录指纹、嵌套仓选择和测试，审查问题清单 `{"issues":[]}`。该工具为指导型，不是自动证明代码无缺陷。`git diff --cached --check` 通过。

旧 `codex无限` 的 `server_info`、`task_open`、`task_status` 均返回 JSON-RPC 32600 / `Session terminated`。因此远程任务 revision 和 checkpoint 未能读取或更新；不能声称远程任务完成。安装、运行进程与真实客户端目录也未在本轮切换或验收。提交 SHA、推送和远端回读以后续同任务 HTML 交接为准。
