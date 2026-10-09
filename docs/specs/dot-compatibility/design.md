# 设计文档：GPTBridge dot 增量兼容

## 概述

覆盖 FR-1 至 FR-8、NFR-1 至 NFR-4。保留现有执行链与合同，只补能力表达、用户说明和工作区详情的真实观察。附件模拟界面用于设计参考；实际应用状态由当前后端提供。

## 技术方案

### 技术选型

沿用 Svelte 5、项目组件/设计 token、Tauri IPC、Rust registry/dispatch 与现有 personal-runtime Store/worker，不新增 Bridge、通用写入工具、认证服务、队列或外部 Agent。

### 架构设计

客户端 → 既有 `/mcp`/认证 → 单工作区 dispatcher 或登记网关 → 原生读取/补丁/持久命令。task_open/checkpoint 与 job/request 保存身份、修订和回执；task 的存在不表示外部 Agent，也不代表命令正在执行。

宿主工作区详情 → 指南/观察组件 → 原 `taskdock_connections`、有界 `taskdock_snapshot`、`taskdock_task` 与作业输出接口。复制动作只复制，刷新动作只读取，不暴露新的执行 IPC。策略说明从已保存 profile 读取，诊断状态来自真实快照；不能伪装成现场 MCP `server_info` 探测。

## 数据模型

`server_info` 增加可忽略 `direct_workspace`：

```json
{
  "version":1,
  "execution_path":"native_tools",
  "external_agent_required":false,
  "durable_bookkeeping":"existing_task_and_request_lifecycle",
  "catalog_refresh":"client_dependent_no_list_changed_notification",
  "security":{
    "execution_isolation":"policy_only",
    "sandbox_enforced":false,
    "read_scope":"workspace_root"
  }
}
```

read_scope 从 Workspace 当前严格读取原子标志获得：true 为 `workspace_root`，false 为 `explicit_external_paths_allowed`。网关构造时启用严格读取；只读工具档位不用于推断该值。示例展示网关值，单工作区按真实标志返回。

UI 复用 TaskRow、连接快照、任务详情及 job 输出。job 状态为 queued/running/exited/cancelled/timeout/queue_timeout/spawn_failed/unknown；exited 的非零退出码仍是失败，任务 active 没有执行中 job 时是待接续。每个快照保留来源/观察时间，缺少客户端证据为“未验证”。

## API 设计

| 入口 | 类型 | 输入变化 | 输出/说明变化 | 对应需求 |
| --- | --- | --- | --- | --- |
| `tools/call: server_info` | 已有接口改造 | 无，原 schema 保留 | 增加可忽略 direct_workspace；旧字段保留 | FR-1、FR-2、FR-5 |
| initialize instructions | 已有说明改造 | 无 | 澄清原生操作、任务记账、真实安全和刷新边界 | FR-2、FR-6 |
| server_info/read_file/apply_patch/exec_command/write_stdin/read_output 的 description | 已有说明改造 | 无；完整 inputSchema 保留 | 仅审核过的说明文字；annotations 保留 | FR-1 至 FR-6 |
| 原 task/open/checkpoint、补丁和命令工具 | 无接口变化 | task/request/revision/hash/session/output 原字段 | 无行为变化，按原结果恢复 | FR-3、FR-4 |
| `taskdock_connections` / snapshot / task / job output | 无接口变化 | 原参数、workspace/task/job 归属 | UI 消费已有真实返回，不新增远程执行能力 | FR-7 |

网关继续给目录中的业务 inputSchema 注入原有必填 workspace_id，再剥离路由参数交给所选成员；该既有模式差异属于基线，不能把网关 schema 与单工作区 schema 混同比对。工具档位差异、插件显式选择标志与本轮变更分别核验。

## 文件结构

- 协议：`src-tauri/src/tools/personal.rs`、`dispatch.rs`、`registry.rs` 及 `workspace.rs` 的只读状态 getter；新增 `src-tauri/tests/dot_compatibility.rs`。
- 界面：`src/routes/workspace/[id]/+page.svelte` 与专属指南/状态模块和相关测试；不覆盖原配置路由或改认证。
- 文档：README 的 dot 章节、PERSONAL、shared-gateway、desktop-plugin README、`docs/dot-compatibility.md`、本三份规格。
- 证据：`docs/gptbridge/dot-compatibility/`；唯一执行真源为 `docs/goals/gptbridge-dot-compatibility-goal.md`，同名 HTML 为伴生产物。

## 设计决策

### 决策 1：兼容用结构证据，说明变化用白名单（FR-1、FR-2）

比较工具名、完整 inputSchema、annotations、暴露档位和旧响应关键字段，不只比较 tool_count。catalog_contract 的 schema 指纹覆盖名称与 schema，顶层 description/annotations 要另比对。可选 metadata 不增加必填参数，也不修改执行行为。

### 决策 2：按真实标志披露读取范围（FR-2、FR-5）

保留单工作区旧显式外部读取和网关 strict_reads。可选元数据读取当前 flag，不按名称、权限档位或网络配置猜测。OS 沙箱、SSE 通知及统一路径加固另立任务，本轮没有伪安全开关。

### 决策 3：恢复继续原请求，不重复执行（FR-3、FR-4）

补丁继续整个文件 hash、写锁与幂等；命令复用原持久 worker。输出持久归属由 job/session/output_ref 决定，公开 read_output/write_stdin schema 不追加 task_id。响应未知先查原回执及效果，不能盲目重放。

### 决策 4：客户端结果和宿主观察分开（FR-6、FR-7）

指南同页常驻，服务可达/任务状态基于真实查询，客户端目录/授权/dot 路由保留未知。listChanged=false、SSE GET 405 保持，无目录推送承诺。P1 不倒逼协议增加执行功能。

### 决策 5：提交、制品和安装同源（FR-8、NFR-4）

启动时冻结 foreign diff；只从本轮拥有片段构造提交与干净候选。唯一 root 协调者执行普通 push、远端读回、App 构建和精确替换，保留旧 App 与任务库。任何 Git 片段无法安全切分时该路径停止提交依赖，其他独立工作继续。

## 测试策略

契约对比先固定基线，再审核白名单说明和 metadata。集成测试使用临时工作区与单测试实例，真实 HTTP/MCP 覆盖 initialize/tools/list/server_info、读→哈希补丁→持久命令→原句柄输出→checkpoint；反例覆盖哈希、幂等、只读、网关目标/路径/归属与恢复。

原 Rust 回归覆盖并发写锁、已完成任务恢复、worker 丢失为 unknown、输出分页和 Unicode 等。前端类型/构建与专属状态测试，真实桌面/375px 页面截图、键盘与空/加载/错误/长路径状态分别记录；原型截图仅作视觉参考。

源码测试、制品、安装、进程、HTTP、宿主 UI 与 dot/@GPTBridge 客户端各自有结果字段。无法运行的真实客户端保持 NOT_VERIFIED，不因上游测试通过写为 PASS。精确命令和证据编号见 tasks 与本轮报告。

## 风险评估

主要风险是工具合同漂移、错误 read_scope、未知命令重放、foreign 误入提交/构建、活动 job 丢失和客户端缓存陈旧。分别用冻结合同、真实 flag、原句柄恢复、差分所有权、活动作业盘点与客户端独立验收控制。发生越权、非白名单差异或无法安全延续活动命令，停止依赖发布步骤，不取消独立只读/文档工作。

## 检查清单

- [x] FR-1 至 FR-8 与数据、接口及验证边界对应。
- [x] 无认证、数据格式或执行链迁移。
- [ ] 按 tasks 完成本轮实际验证和安装回读。
