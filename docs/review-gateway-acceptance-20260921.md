# 0.3.1 代码审查与多仓库共享入口验收

日期：2026-09-21。基线提交：`545958e36109adfbbff572dadf09937379f48308`。

## 审查发现与修复

| 问题 | 实际证据 | 修复 |
|---|---|---|
| 缺省 upstream 列表被导入成 null，无法被桌面解析 | 原生 AppData 回归实际失败，报 expected a sequence | 新导入不写入 null；已有个人版 null 配置兼容读取，不重导或改旧配置 |
| task_open 后续原文校验失败，已经创建任务并遗留回执 | 失败请求后任务数量为 1，预期 0 | 创建、绑定、原文事件与幂等回执使用一个 SQLite 事务；无效请求零落盘 |
| 已完成任务仍可提交新补丁 | completed 任务的 late.txt 实际被创建 | 新补丁检查任务完成态，返回 TASK_COMPLETED；旧成功请求仍可读取原回执 |
| 同一补丁重复修改同一文件会丢掉第一段 | 实际得到 one/TWO，而不是 ONE/TWO | 后段以事务中已暂存内容为基线；后段冲突时整单不落盘；保留原 NOT_FOUND 错误码 |
| 网关复用旧只读接口，显式绝对路径可读其他仓库 | 选择 A 后读取 B 绝对路径实际返回 B 内容 | 网关模式对 canonical 读取根目录做校验；单仓库旧行为不擅自改动 |
| 恢复请求可报告原文已捕获但未写事件 | 补充持久事件回归 | 显式提交原文必须在事务内保存才返回捕获成功；重复 request ID 不重复写 |

同时补充 HTTP 在途请求 32 个名额和 2MiB 请求体限制，避免无限创建阻塞执行工作；通知请求返回 202 空正文。许可与路径检查不被网关绕过。

## 单入口多仓库

配置字段：入口 `runtime.gateway_workspace_ids`，默认空。界面“一个入口 · 多个仓库”只保存明确选中的成员，需确认该入口认证可访问全部成员。

每次工具调用使用 `workspace_id`，无全局当前仓库；成员保留自己的 ToolContext、任务库、命令输出、工具档位和路径/执行策略。未知仓库、遗漏仓库 ID、跨仓库任务 ID 和改写 history 的 workspace_root 均拒绝。成员不启动独立监听或隧道，因此一条 FRP 只需转发入口本地端口。

运行中的共享入口不允许修改成员路径、授权或拓扑，不自动停止/重启。删除仍被引用的成员也会拒绝。外部 MCP 聚合暂不纳入共享入口，启用的外部 MCP 会明确触发配置拒绝，不静默遗漏工具。

## 最终验证

命令：`python3 scripts/selftest_personal.py --suite all`。

- 私有原始回执：`.artifacts/selftest/1789989068057911000/result.json`。
- **13 个阶段全部退出 0**。
- **完整 Rust 库：205 passed、0 failed、0 ignored**；比基线 182 项多 23 项，定向套件是其子集，不重复累计。
- 多仓库路由及真实 loopback HTTP、Bearer 鉴权、OAuth 发现/无效 token、并发同路径读取、任务归属、只读成员、运行中配置守卫、HTTP 限流和体积上限通过。
- Python 配置/工具链回归、运行时与 worker 进程回归、Svelte 真实组件编译、类型检查、前端构建和 macOS 桌面开发二进制构建通过。
- 测试前后源码指纹一致：`09f67541a7987f6a43686b4d09ba7702c96b180fb8f5d5ae46fd644c882d8330`。

此外，`python3 scripts/personal.py check-config` 已实际运行退出 0，使用构建好的桌面二进制解析已有个人配置：`profiles=1, native_deserialization=true, configuration_written=false, services_started=false`。不是只比较 JSON 外观，也未输出本地凭据。

## 失败回执保留

初始审查回归 4 项全部失败：`.artifacts/checks/review-red-1789988119715553000/result.json`。

重复文件段复现：`.artifacts/checks/review-sections-red-1789988577998994000/result.json`，5 通过、1 失败。网关首次真实回归：`.artifacts/checks/gateway-first-1789988478175688000/result.json`，10 通过、1 失败，暴露外部路径读取问题。

完整回归曾出现 NOT_FOUND 被改成 PATCH_FAILED 的兼容差异：`.artifacts/selftest/1789988770063181000/result.json`，203 通过、1 失败；已修回原约定并重跑全部。没有修改既有测试来掩盖该差异。

## 尚未执行与适用边界

本轮没有改变实际 FRP、没有停止/重启/替换旧 MCP，没有启用用户真实多仓库入口。测试只启动自有临时 loopback 服务并关闭，不是公网 FRP 或真实用户 OAuth 登录的验收。原生 GUI 交互、Windows 实机仍未验证。

工作区根包含其内部子目录；嵌套根不是独立安全域。命令执行仍为本地策略约束，不是 OS 沙箱。文件协作锁不覆盖另一套旧 MCP 或外部编辑器；未知外部副作用不自动重放。

辅助 code_insight 实际运行但退化为有界文件/符号清单，返回 `impactAnalysis=false`；不将其称为完整调用图审查。本轮结论来自实际源码和红绿回归。
