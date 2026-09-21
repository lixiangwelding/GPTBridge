# Coding Tools MCP Personal

版本：0.3.3（个人 fork 版本号，不表示与其他上游同号版本实现相同）。

0.3.3 修复 MCP 事件流 GET 响应：客户端以 `Accept: text/event-stream` 访问 `/mcp` 时返回 `405`，普通健康探测仍可读取 JSON 版本信息。修复与验证记录见 [0.3.3 发布说明](docs/release-0.3.3.md)。

0.3.2 新增 [本机 Skill 目录桥接](docs/local-skills.md)：自动发现现有项目/全局 SKILL.md，通过同一个插件按需搜索和引用。`$技能名` 是消息发送后的文本调用约定，不是 ChatGPT 输入框原生下拉；脚本和外部 MCP 不会因读取技能而自动执行或连接。

0.3.1 支持 **一个 MCP 地址、一条 FRP 隧道、多个仓库**，通过个人版配置页勾选共享成员。详见 [共享入口操作说明](docs/shared-gateway.md)。默认仍为单仓库，不修改旧服务、旧连接或旧 FRP。

## 直接使用

本版使用独立应用名、bundle ID、Windows 单实例标识、配置目录和更新仓库。旧版继续运行，不需要停止或替换。

```bash
cd /path/to/coding-tools-mcp-personal
npm ci --no-audit --no-fund
python3 scripts/personal.py build
python3 scripts/personal.py import-config
python3 scripts/verify_personal_config.py
python3 scripts/personal.py check-config
```

`build` 只构建，`import-config` 只复制配置，均不会启动 GUI、监听器或隧道。已存在个人配置时导入会拒绝覆盖；这不是需要反复执行的日常命令。可通过 `--source /absolute/path/to/profiles.json` 指定源文件。核验只输出哈希、配置数量及布尔状态，不输出凭据。

`check-config` 使用真实桌面端的配置解析器，只读检查已保存的个人配置和共享成员。0.3.1 兼容早期导入器产生的 `upstream_mcps: null`；新导入不再生成该字段的无效空值。无需删除配置或重导入来升级。

明确准备启用个人版时再运行：

```bash
python3 scripts/personal.py start
```

个人配置在仓库 `.personal-home/data/profiles.json`，开发制品在 `.artifacts/cargo/debug/`。macOS 开发二进制名为 `coding-tools-mcp-personal`；这不是签名 DMG 安装包。

导入保留工作区及其余配置/本地凭据，分配从 38766 开始且不与源配置重合的端口；关闭自动启动、隧道及上游 MCP 自动连接，并清空指向旧安装的外部启动命令。端口尚未预留，启用前仍需检查是否被其他程序占用。旧公网连接不会自动切到个人版；为个人版配置独立连接，不把正在使用的旧隧道指向新端口。

## 不再复制初始化模板

连接个人版后，正常描述目标即可，例如“修复这个接口并补测试”。服务在 MCP initialize instructions 和工具说明中告诉 Agent 使用 `task_open`，不要求用户粘贴历史初始化模板。

新任务：`task_open(goal=目标)`；没有客户端会话元数据时由 Agent 提供稳定 `request_id`。恢复：`task_open(task_id=之前的ID)`。查看：`task_status`。每步完成：`task_checkpoint`，只提交步骤增量和证据。Agent 保留并使用返回的 ID，用户不需复制全套参数。

同一对话恢复可利用客户端实际传来的会话元数据。新对话无任务身份时先列任务并选择正确目标；不能把全工作区“最近一个任务”直接认领。原 `history_session_*` 保留给明确需要的旧 Markdown 档案，不再是所有新任务的强制前置。

服务只能记录工具显式收到的目标、原文和结果；不能读取未传入的聊天消息。用户不复制模板，不等于服务可以自动读取完整聊天历史。

## 共享主目录并发，不创建 worktree

读代码、分析和准备补丁可以并行，不按目录长期锁定任务。受管文件修改只在哈希校验和落盘时占用写锁。同文件旧补丁返回 `STALE_FILE`，Agent 重读最新内容、保留其他任务修改后重新提交；不按旧全文覆盖。相同请求 ID 返回原回执，不重复应用。

命令初始上限：全局同时执行 8，重构建 2，排队与运行合计 32。读任务可以并发；会修改源码的命令默认 `mode=write`，持有源码写锁。`mode=build` 使用共享源码锁和输出资源锁；默认公共输出串行，确认独立输出后用不同的 `resources`。这些是资源预算，不是已测出的性能最优值。

所有 Agent 写入走受管入口；格式化、代码生成、Git 写入和脚本写源码也按写操作协调。不能把实际写命令标成 read 来追求并发。共享构建验收期间相关源码保持稳定。外部 IDE、未受管脚本及另一套旧 MCP 不保证遵守个人版的锁：验证个人版时不要让两套服务同时修改同一批源码。

## 超长任务与故障恢复

任务进度存于独立 SQLite，使用稳定步骤 ID、原子 revision 校验和请求幂等回执。千步任务默认仅返回未完成项和摘要，原步骤可分页读取，不要求把全历史塞回上下文。

非交互命令由独立 worker 执行，保存 job ID、输出、退出码和资源状态。客户端断线或提交进程退出后，可查询原 job，而不是重跑命令。长命令显式使用 `durable=true`，最大单次命令时限 24 小时；任务可由更多步骤组成，不被一条命令时限限制。

worker 或主机崩溃时，不承诺任意进程指令级续跑。监督状态丢失的 job 标为 `unknown`，原请求不自动重放；核实外部效果后明确安排后继步骤。未确认 job 不允许冒充已验证完成，可保留 blocked 或 completed_unverified 状态。当前没有自动裁决未知数据库/发布结果的功能。

恢复接着最新工作区执行，**不恢复整仓旧快照，不回滚其他任务成果**。测试证据由调用方声明，工具只检查字段/状态，不把“有证据字符串”当作业务正确性证明。

交互式旧 TTY 会话不享有持久 task 恢复；绑定 task 的命令要求非交互 durable 路径。Windows 批处理引用方式被明确限制；本轮真实运行验证平台为 macOS，未完成 Windows 实机验收。

## 一条命令自测

```bash
python3 scripts/selftest_personal.py --suite all
```

测试使用 `.artifacts/selftest/<run>/` 中的独立状态，不使用旧配置。覆盖 Python 启动/配置核验、Rust 任务运行时、真实 worker 子进程、MCP 实际 dispatcher、全量 Rust 旧功能回归、前端类型检查/构建、桌面编译。保留每阶段原始日志、退出码和测试前后源码指纹；失败不会改写成成功。

中断后仅在源码与 suite 相同时可使用 `--resume .artifacts/selftest/<run>/result.json`，不会跳过变化输入。测试脚本为 GUI 启动的简化 PATH 补充已安装的工具目录，仅作用于测试/构建子进程，不修改系统 PATH 或旧服务。

`.personal-home`、`.artifacts`、SQLite、依赖、构建缓存和本地计划工具目录均不进入 GitHub。测试通过不代表旧连接已经切换，也不代表签名安装包、实际 GUI 手动操作、跨平台或生产发布验收已完成。

## 源码来源与设计参考

- 直接 Rust/Tauri 基线：`mybolide/coding-tools-mcp`，提交 `8e76bd06cce5519acad624cce209cd03a6f7bceb`；Git 历史保留。
- 原始 Coding Tools MCP：<https://github.com/xyTom/coding-tools-mcp>。保留其许可证与 NOTICE，参考有界输出、版本校验、多文件补丁和权限契约，未把 Python 上游整套覆盖到 Rust 桌面端。
- 持久任务生命周期参考 MCP 2025-11-25 Tasks：<https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/tasks>。本版为常规自定义工具，并未宣称实现标准 Tasks 协商协议。
- 状态与协作锁采用项目已有 Rust 生态依赖及 SQLite，不新增 Redis、队列服务器、worktree 或第二套 IDE。

个人版的开发说明与实现以上述源码和可复现测试为准，旧 README 仅作为上游功能参考。
