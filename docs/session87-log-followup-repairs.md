# Coding Tools MCP 个人版：session87 日志审计后续修复交接

> 2026-09-22 后续启用更新：本轮已重新打包、保留配置安装、退出旧进程并启动新进程；本地与公网均通过 33 工具完整输入 Schema 比对。ChatGPT 开发模式连接已实际点击刷新，客户端管理页从 26 项变为 33 项。按用户补充要求，真实对话验收必须使用“聊天”，误开的“工作”验收已停止且不计入结论。“聊天”验收结果见文末后续记录。下文原“未安装/未刷新”描述保留为前一轮交接时点。

日期：2026-09-22。工作目录：`/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal`。
用户授权：按上一轮日志审计建议完成修复，能做到哪做到哪。

## 当前结果

已修改 17 个源码、脚本和测试文件；本文件是新增交接文档。main 基线仍为 `21c429ce0d95f5a553f78140dd9b0a2e19262696`，本轮未提交、未推送、未安装替换、未重启共享 MCP/FRP，也未更改认证、活配置、并发上限、外部路径权限和补丁保护。版本号仍为 0.3.4，新增构建只是候选二进制，不代表当前运行应用已经采用本轮修复。

最终强化自测 14/14 阶段通过；桌面 Rust 289 项单元测试 + 83 项集成测试全部通过；Python 35 项通过。持久运行时、MCP 协议、权限、Skill、两组现有前端测试、Svelte 检查、前端构建及桌面构建通过。Svelte 为 0 errors / 0 warnings；Rust 存在既有未使用函数等警告，不声称零警告。定向子集不与全量重复累加。

## 已完成的修改

### 历史会话隔离与正确接续
`src-tauri/src/tools/history/storage.rs`：只从显式当前会话生成焦点、待办和近期改动，其他会话仅作为只读检索参考。待办取最新轮次检查点快照，已解决事项不从旧检查点回流。修订号只在同一轮内部比较，按轮次首次出现顺序保留最新修订；旧轮次晚到的更正也不会替换较新的用户请求。新增 6 项回归覆盖这些边界。原 Markdown 保留，未删除、回滚或重写用户的其他历史。

### 历史检索减少重复计算、增加分段证据
`src-tauri/src/tools/history/mod.rs`：读取原文后只构建一次 manifest，使用编号映射定位文档，只为返回页生成 Unicode 摘要。保持排序、哈希与有界分页约束。搜索输出扫描、manifest、排序和页面生成耗时；bootstrap/checkpoint 输出锁等待、初始扫描、归档更新和派生状态刷新耗时。
这消除了已确认的重复工作，但没有证明先前单次 24.575 秒长尾的唯一根因，也没有宣称线上长尾已经消失。

### 持久作业归属与执行元数据
`src-tauri/src/tools/dispatch.rs`：轮询响应保留持久作业自身 task_id；没有归属的 null 同样是权威值，不让调用方重标任务。保留稳定请求 ID 下的去重回执；不把相同目录推断为相同任务。`src-tauri/src/tools/exec.rs` 补齐 durable 响应的 filesystem_scope=workspace 与 child_process=true，原有范围校验不变。
当前 ChatGPT 仍缺 task 工具/参数，因而“真实当前对话的任务绑定使用率”尚未闭环；本轮验证发生在隔离真实 worker 测试中。

### 审计耗时边界
分发器新增 dispatch_ms / audit_ms / total_ms；慢调用输出不含请求正文和凭据的计时日志。不递归写第二条审计，不改变原操作成功/失败结果。计时明确不包含网络、排队和持久作业完整生命周期，不能当作 HTTP 端到端性能。

### Git 路径兼容
`src-tauri/src/tools/git.rs`：仅 git_status 接受位于当前工作区内部的绝对路径；按路径组件判断范围，随后走原规范化/软链接校验。外部目录、同名前缀兄弟目录、父级跳转与软链接逃逸仍拒绝。没有拓宽通用写权限。

### Skill 返回精简
`src-tauri/src/tools/skill_discovery.rs`：目录预览上限 6 条、条目预算 2 KiB、描述 192 字节；server_info 保留数量、目录版本及检索指导，不重复展示 Skill 列表。显式 list/search/read/invoke 能力与完整目录保持不变，正文仍按需读取。没有新增缓存或关闭安全过滤。

### 工具清单校验器
新增 `scripts/check_tool_catalog.py` 与 8 项 Python 测试：离线比较服务器导出与客户端观测，区分完整 Schema 一致、仅字段一致、明确不一致和无效输入。不会联网、重启或改配置，不把文件比对通过冒充当前会话已刷新。
已用新编译二进制 `--personal-tool-contract` 实际导出，并与本对话真实加载的 26 工具字段清单比对：缺 7 个工具，apply_patch 缺 request_id / expected_hashes / task_id；exec_command 缺 durable / mode / request_id / resources / task_id；检索工具也缺部分字段。比较器返回 mismatch，退出 1，是预期发现，不是测试失败。

### 完整自测补漏
发现旧标准全量只跑 Rust lib，未纳入 src-tauri/tests；检查器还未配置独立 worker，集成测试会误把自身作为 worker。修复 run_checked.py 的显式隔离 worker 路径并加 3 项测试，将全部集成测试纳入标准 full-rust-regression，且将 src-tauri/tests 纳入前后源码指纹。
旧测试针对 26 工具、legacy harness 或缺少补丁保护参数，已按当前已有契约迁移：明确选择 legacy 模式测试旧会话，默认 durable 单独验证；补丁正例传稳定 request_id 和实测文件哈希，并保留缺请求 ID、缺哈希、陈旧哈希、保护路径及删除确认负例。没有通过删除测试或弱化生产保护来清零错误。

## 证据与构建

完整回归：`.artifacts/selftest/1790062704090386000/result.json`。
最终回执和逐文件 SHA：`.artifacts/session87-followup/final.json`。
工具服务端导出：`.artifacts/session87-followup/native-contract.json`。
客户端实际字段观测：`.artifacts/session87-followup/client-fields-observed.json`（字段名，不是假造的完整 Schema）。
对比结果：`.artifacts/session87-followup/catalog-comparison.json`。
源码总指纹：`3b2c7c488c5b514aa1226d737796f4d5e2b7a687de06db2d23c4be33ffa8c5d3`，测试前后及最终回读一致。
候选二进制：`.artifacts/cargo/debug/coding-tools-mcp-personal`。
候选 SHA256：`637c1367747285c0f23f1a98fc74bfa66f22be7beeb61302353e8181590599ce`。
工具 Schema SHA256：`0eaedfaf85246c8e14e6a5c63517c38653cf7beff2d7189010de381de130cc79`，工具数 33。
`git diff --check` 通过；当前代码和实际 diff 已人工审查。Probe/图谱执行被平台拦截，没有宣称完成 GitNexus 图谱影响分析。

失败回归记录保留于 `.artifacts/checks/session87-*` 和中间 selftest；最后一次成功只对应上面的最终指纹。可选真实历史 fixture 未证明已启用，不能用其默认通过状态声称 live 历史验收。

## 仍未完成

P0 客户端工具目录刷新：这是最重要的剩余阻塞，本轮不能在工具参数中强制改变 ChatGPT 当前插件清单。开发模式连接按 OpenAI 官方 Refresh metadata 流程刷新连接、核对字段并在新对话复验；已发布插件应走对应官方更新流程，而非假设同一 UI 流程。此项未执行，原生 $ 下拉也不在本轮完成范围。

发布启用：本轮仅候选源码与构建。先回读最新 Git/文件 SHA 和当前共享作业状态，在明确空闲窗口按既有保留配置升级器打包、备份、安装及切换；再确认实际运行二进制身份和真实对话行为。不得用未经核验的旧 .app 目录或相同版本号冒充新制品。提交和推送按后续明确发布流程处理，不自动整仓暂存。

性能验收：还需新实例的冷启动、多对话并发、历史写锁竞争及真实远程请求样本。已有分段计时可定位实际瓶颈；不能承诺任意并发或所有长尾清零。

## 接续验证入口

从个人仓 main 目录执行 `python3 scripts/selftest_personal.py --suite all`。修改过源码/测试时必须新跑，不复用旧指纹结果。
工具比对命令：`python3 scripts/check_tool_catalog.py --expected <服务端导出.json> --observed <新对话真实工具清单.json>`；0=完整 Schema 匹配，1=不一致，2=输入无效，3=仅字段一致未验证完整 Schema。
任意授权写入仍保留 request_id、expected_hashes、工作区边界和幂等保护。不要为适配旧客户端删除这些保护。

## 后续安装、运行切换与客户端刷新（2026-09-22）

用户授权继续完成上述安装启用及客户端更新；本轮仍只操作个人版独立仓与个人版应用，不提交、推送或发布其他业务。

- 源码前后指纹仍为 `3b2c7c488c5b514aa1226d737796f4d5e2b7a687de06db2d23c4be33ffa8c5d3`，与此前 14/14 阶段通过的输入完全一致。没有重复把旧回归算成本轮新增测试。
- 已执行 `run_checked.py --name session87-activation-bundle --timeout 540 -- npm run tauri -- build --debug --bundles app`，打包成功。重新打包包含桌面资源嵌入，因此安装制品以本次 SHA 为准，不沿用未打包候选的 SHA。
- 安装路径：`/Users/didi/Applications/Coding Tools MCP Personal.app`；实际二进制 SHA-256：`dcc37a568d49d50eb87d902d73442da0d7c917e822a6a98ecf0417a604f065f0`。版本号仍为 `0.3.4`。
- 保留配置升级器已备份旧应用和私密配置，回执为 `installed_app_files`，安装时配置逐字节一致。备份根：`/Users/didi/Library/Application Support/coding-tools-mcp-personal-backups/20260922T075840Z-1790063920132876000/`。
- 切换前只读确认持久作业 `queued/running=0`，旧进程无 worker 子进程、最近约 95 秒无工具调用。通过应用界面停止 MCP、退出旧进程，再打开新应用。旧 PID `44044` 已退出，新 PID `27738` 的映射文件与实际安装路径一致，MCP 与原 FRP 恢复，Actions 继续停止。
- 重启后配置 JSON 键顺序变化；解析后的全部配置值与备份相等，未更改工作区、OAuth、端口、FRP 或权限策略。未带认证的本地 MCP 请求仍返回 HTTP 401。

### 实例行为与有界性能样本

本地 `127.0.0.1:28766/mcp` 和既有公网端点均实际 initialize / tools/list 成功；33 个工具的完整输入 Schema 与安装包原生导出一致，校验器返回 `schema_match`。这项比对是服务端协议层结果，与 ChatGPT 客户端观察分开记录。

真实实例验收涵盖：工作区内绝对 Git 路径成功、外部路径拒绝、server_info 不展开 Skill 目录、Skill 四工具实际调用、两个不同任务隔离、持久作业轮询归属、稳定请求 ID 重放返回同一作业、未绑定作业保持无归属。两条协议夹具任务均已通过检查点完成，无业务文件写入。

历史检索读取现有 88 份档案，关键词“修复”命中 68 份，每次 limit=1。五个样本均只构建一次 manifest、生成一条摘要：两次串行本地 HTTP 约 202.7 / 176.6 ms；一次公网约 725.3 ms（服务端分发约 207 ms）；两个并发只读本地样本均约 185.9 ms。这是低并发有界样本，没有强制制造历史写锁竞争，不证明先前 24.575 秒长尾的唯一原因或彻底消失。

### ChatGPT 客户端与“聊天”模式边界

实际打开个人插件“codex无限”的开发模式管理页，核对连接为 `https://codingtools.ssh.ddtgm.lol/mcp`，刷新前确为 26 条操作、补丁参数陈旧；点击该页“刷新”后出现 33 条操作和新的补丁保护参数，7 个 Skill/Task 工具全部可见。未更改连接权限或认证。

官方参考：[Refresh metadata](https://developers.openai.com/plugins/deploy/connect-chatgpt#refresh-metadata)。插件页“在聊天中试用”在当时账户实际进入了“工作”；发现后已停止。用户明确要求“聊天”，随后通过单选按钮核对“聊天=1、工作=0”、输入框“问问 ChatGPT”，重新提交只读验收。工作模式结果不作为聊天验收通过证据。

聊天验收状态：待本次真实聊天返回，尚未写作通过。

### 后续回执

- 总回执：`.artifacts/session87-activation/final.json`。
- 打包回执：`.artifacts/checks/session87-activation-bundle-1790063782885005000/result.json`。
- 实例身份：`.artifacts/session87-activation/process-identity.json`。
- 本地/公网工具目录：`.artifacts/session87-activation/local-catalog.json`、`public-catalog.json`，原生安装导出 `installed-contract.json`。
- 真实协议检查与计时：`.artifacts/session87-activation/acceptance.json`。
- 客户端刷新证据：`.artifacts/session87-activation/chatgpt-refresh.json`。

所有私密回执、运行数据和构建物保持在 Git 忽略路径。原源码变更未提交、未推送；没有安装签名版、Windows 验收或高负载性能承诺。
