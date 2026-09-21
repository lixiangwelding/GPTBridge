# Coding Tools MCP Personal：多会话性能优化与代码审查

任务日期：2026-09-21（UTC）。基线提交：`a2c6f2ab0683182b635c77380f597bcd5bd596c7`。本记录对应同目录 `performance-multichat-2026-09-21.json`。

## 交付范围与方案

本轮只修改个人版仓库的运行时、MCP 请求准入、Skill 元数据读取和对应测试。保留当前工作区结构、写入互斥、任务身份、请求幂等、未知结果不自动重放、鉴权、SQLite WAL + FULL 持久性。没有修改用户配置、安装目录、正在运行的服务或 FRP。

采用“减少重复工作 + 有界并发隔离”，而不是直接提高所有并发上限。

| 改动 | 原先的开销/问题 | 本轮实现 |
|---|---|---|
| 任务列表 | 50 条任务最多打开 51 个数据库连接；读取日志后又丢弃正文 | 同一请求复用一个连接和 prepared statements；列表仅取状态及输出引用，补排序/活跃任务索引 |
| 命令等待 | 每 20/30ms 重新连接数据库并重复读取日志 | 请求内连接复用、只轮询元数据；20→40→80→160→200ms 有界退避，返回时读取输出 |
| Worker 监测 | 取消检查每 30ms 重新打开连接 | 监测循环复用连接；不跨 sleep 持有读事务；30ms 取消/超时检查周期不变 |
| HTTP 准入 | 长命令和长轮询可占满全部 32 个请求名额 | 每监听器总量仍为 32；执行/等待类最多 24，为其他请求保留 8 个名额；拒绝返回 429 + Retry-After |
| Skill 搜索 | 每次扫描都重新读取全文、解析 YAML 并计算 SHA | 保留逐次目录和规范路径检查；按文件身份、长度、mtime/ctime（纳秒）复用元数据；每 Catalog 最多 1000 项，共享克隆缓存，不保存正文 |
| 工具使用提示 | 用 exec 代替文件读取会进入默认写锁 | 提醒优先使用原生文件/搜索工具；只有真正只读且 Schema 暴露时才声明 read，不能把写操作标记为读 |

命令执行名额仍为每工作区 8，重构建仍为每工作区 2，排队与运行合计仍为 32。保留这些保护，避免多会话重编译同时挤占 CPU/内存。

## 实测结果

同一台本机、debug 构建、隔离临时状态目录。基准先于生产代码修改加入，并保存了修改前的真实结果。耗时断言不作为 CI 门槛。

| 场景 | 修改前 P50 / P95 | 最终 P50 / P95 | P95 降低 |
|---|---|---|---|
| 8 并发，每次列 50 条任务，共 80 请求 | 81.15 / 87.49 ms | 3.29 / 5.61 ms | 93.59% |
| 8 并发，201 个 Skill，预热后共 24 次扫描 | 707.92 / 728.28 ms | 50.94 / 57.78 ms | 92.07% |

这些数字衡量本地热点，不是网页 ChatGPT 的整体回复速度，也不是 FRP、网络或模型推理性能。没有声称 8 个以上会话能线性扩展；冷启动首次读取仍需扫描与解析。

机器可读记录包含前后测量原值及相对日志路径。日志留在 `.artifacts/`，不会把私有运行状态、账号配置或完整日志提交到 Git。

## 审查中发现并修复的完成时序问题

复跑时 `cancellation_and_timeout_stop_only_the_owned_command` 曾失败：工具已读到终态，但 source 锁尚未释放。旧路径在 `execute()` 写入终态之后，才由调用方释放资源；后续可选事件记录也会延长这个窗口。更快的状态读取使该竞态更容易暴露。

修复后正常路径严格为：停止本次拥有的子进程组 → 关闭/收集管道 → 持久化日志 → 释放 source/resource/command 名额 → 写入终态。spawn 失败先释放预占，再发布失败；异常返回仍由 OwnedCommand 清理子进程后释放资源。没有通过删除锁、延长测试等待或降低断言来消除失败。

新增 16 次真实子进程终态/资源释放检查，并对原失败的取消/超时用例额外独立复测 6 次，全部通过。

## 最终验证

- `python3 scripts/selftest_personal.py --suite all`：14/14 阶段通过。
- 桌面 Rust 核心：245 项通过；独立 runtime：35 个测试通过（不重复计入 helper 子进程和筛选子集）。
- Python 工具脚本、MCP 契约、权限策略、工具目录、Svelte 检查、前端构建、桌面编译均通过。既有编译警告未被宣称清零。
- 实际回环 HTTP 测试：24 个等待请求占用时，第 25 个等待被 429 拒绝，另一个仓库的读取仍成功；未鉴权请求仍为 401；取消仍可送达。
- 总容量拒绝会归还已取得的等待名额；HTTP handler 取消后，两个名额都保持到真正的执行闭包结束。
- 缓存测试覆盖克隆共享、容量上限、同长度修改且恢复 mtime、原子替换、无效 YAML、软链越界、正文不缓存，以及原有分页 SHA 校验。
- SQLite 连接复用测试验证另一连接提交能被观察到，未持有长读事务；缺失任务、未知结果、非零退出、输出截断语义均保留。

最终源码指纹：`ec6cdf5a2361b61eeac8cffee9ed31825fe79b44b4f754533932f2f7144c337d`。
最终自测回执：`.artifacts/selftest/1790004476748949000/result.json`。
汇总回执：`.artifacts/performance-59/final-verification.json`。
首次 HTTP 红灯回执：`.artifacts/checks/perf59-http-red-1790003661924736000/result.json`。
时序问题失败回执：`.artifacts/selftest/1790004282999672000/result.json`。

## 代码审查结论与边界

已检查实际 Git diff、调用路径和新增未跟踪测试文件，并结合上述真实回归结果完成语义审查。本轮范围内未发现未处理的阻断项。

`probe code_review` 是证据增强的指导工具，提供真实 Git 差异，不是独立第二模型审查。GitNexus 调用图不可用（code_insight 退化为文件证据，CLI 不在允许列表），因此没有声称完成图级影响分析；采用源代码调用路径与测试覆盖核对。

需要保留的兼容性说明：任务列表条目现在有 `output_loaded=false`，`stdout_truncated/stderr_truncated=null` 表示“尚未检查输出”，并非空日志或未截断。单任务查询与命令返回仍加载正文并提供布尔截断标志；需要日志时按 output_refs 读取。

## 生效与剩余边界

本记录覆盖源码修改、测试与审查，Git 提交信息以对应提交为准；没有替换现有 `.app`、创建安装包或重启 MCP。只有后续将该提交构建并安装、重启相应进程后，正在使用的插件才会获得这些优化。现有配置文件不用迁移。

工具目录需要与服务器 Schema 一致。本轮连接器曾显示旧参数 Schema：apply_patch 未暴露已要求的 request_id，exec 也未暴露 mode。不能凭空传入未暴露参数；本轮使用受管理、排他写入的命令通道，并核对写前哈希保存修改回执。后续使用应同步连接器工具声明，否则 exec 仍默认写锁，不能期待所有 shell 命令自动并行。

Windows 未实际运行验证，非 Unix 平台 Skill 元数据缓存主动退回每次读取。网络文件系统的时间戳精度和性能未验证。Legacy history-session 文件扫描和归档格式未改动。轮询检查间隔上限为 200ms，这是减少空转的取舍，不是端到端时延保证。

复验命令：

```bash
python3 scripts/selftest_personal.py --suite all
python3 scripts/run_checked.py --name multichat-jobs --timeout 300 -- cargo test --manifest-path personal-runtime/Cargo.toml --test performance benchmark_multi_conversation_job_list -- --nocapture
python3 scripts/run_checked.py --name multichat-skills --timeout 300 -- cargo test --manifest-path src-tauri/Cargo.toml --lib benchmark_multi_conversation_skill_catalog -- --nocapture
```
