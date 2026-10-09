# 任务清单：GPTBridge dot 增量兼容

## 概述

执行本轮用户已授权的 P0/P1 兼容实现、测试、commit/push 与本机应用替换。realpath 为 `/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal`，branch 为 main，基线 `17f47d3d261aa23f9e8d2c11d269438e000bceba`。唯一 Git/安装协调者为 root；文档清单不代表测试、推送、安装或客户端签收已经完成，实际状态由 Goal 与交付报告更新。

## 交付物清单

原生能力说明、六项 description 白名单、完整合同/测试证据、工作区同页指南与真实状态、用户文档、当前 Goal md/html、干净提交/远端 SHA、本机 App 和可恢复旧 App。foreign diff 单独冻结，禁止整仓暂存、reset、clean 或 force-push。

## 数据库改动

无数据库改动；无 DDL、历史回刷或数据格式迁移。任务/命令 SQLite、配置与恢复句柄沿用原路径。替换与回滚不得删除任务库。

## 接口与消息

| 接口 | 类型 | 入参变化 | 出参变化与兼容 |
| --- | --- | --- | --- |
| `tools/call: server_info` | 已有接口改造 | 无，原完整 schema 保留 | 可忽略 `direct_workspace`，所有旧字段保留 |
| initialize | 已有说明改造 | 无 | instructions 说明更新；capabilities 的 listChanged 保持 false |
| 六个原工具 description | 已有说明改造 | 无，schema/annotations 不变 | 仅白名单说明文字，不改行为 |
| task/patch/exec/output 与 taskdock IPC | 无接口改造 | 无 | UI 使用原真实返回，继续原归属校验 |
| MQ | 无消息改动 | 无 | 无 topic、消息对象或发布顺序改动 |

## 全局配置改动

无全局配置改动。无端口/认证/网关成员/工具档位/命令策略/任务保留配置迁移，无新的环境变量。`direct_workspace.security.read_scope` 读取既有实际 strict-read flag，不能当成新的可配置开关。

## 任务列表

### 阶段 1：基线与所有权

- [x] 1.1 冻结 Git、运行实例、各模式目录与 foreign diff，建立工具名/schema/annotations/暴露规则快照。
  - **证据块**：`registry.rs:input_schema/catalog_contract`；`mcp/gateway.rs:WorkspaceHub::new` 给业务目录增加原有 workspace_id；`server.rs:initialize_result` 保持 listChanged=false。
  - **涉及文件**：本轮 Goal、契约/ownership 证据；不修改 foreign 片段。
  - _需求: FR-1, FR-5, FR-6, FR-8_ ｜ _设计: 决策 1、5_

### 阶段 2：说明、文档与宿主界面

- [x] 2.1 增补可选 direct_workspace，澄清原生工具和持久记账，只改审核过的说明文字。
  - **证据块**：`dispatch.rs:server_info`；`personal.rs:INSTRUCTIONS`；`workspace.rs:strict_reads/resolve_read_path`；`gateway.rs:restrict_reads_to_root`。
  - **涉及文件**：personal、dispatch、registry 精确片段、workspace 只读 getter、新增 dot_compatibility 测试。
  - _需求: FR-1, FR-2, FR-5, FR-6_ ｜ _设计: 数据模型、决策 1、2_
- [x] 2.2 补双路径用户说明与真实 schema 示例，区分读取范围、刷新、未知结果和恢复。
  - **证据块**：`personal_schema.rs`；`registry.rs:input_schema`；`personal.rs:job_poll/job_output`；`jobs.rs:job_status`。
  - **涉及文件**：README 新增 dot 章、PERSONAL、shared-gateway、desktop-plugin README、dot-compatibility 用户指南和本规格。
  - _需求: FR-2, FR-3, FR-4, FR-5, FR-6_ ｜ _设计: API、决策 3、4_
- [x] 2.3 在工作区详情同页实现指南、真实连接/任务观察与回执，不新增执行链。
  - **证据块**：原 `commands/taskdock.rs`、taskdock 查询类型与 JobLogDialog；创建目标不等于执行。
  - **涉及文件**：工作区页面、指南/状态组件与专属测试、截图与视觉评审。
  - _需求: FR-7_ ｜ _设计: 架构、决策 4_

### 阶段 3：精确验证

- [x] 3.1 运行规格结构检查、真实 schema 示例检查和契约差分，仅接受说明/可选 metadata 白名单。
  - **证据块**：`.mcp-probe-kit/bin/probe schema check_spec` 已确认只读工具，规格位于本目录。
  - **涉及文件**：check-spec-result 与契约差分证据。
  - _需求: FR-1, FR-2, FR-5, FR-6_ ｜ _设计: 测试策略_
- [x] 3.2 用单测试实例/临时工作区完成 HTTP/MCP 直接闭环和反例，并运行原回归。
  - **证据块**：`personal_tests.rs` 的 two_tasks_same_file/repeated_patch/durable_dispatch；`gateway_tests.rs` 的 cross_repository_absolute_path/no_default/readonly_member；runtime jobs/concurrency_recovery/output_paging 测试。
  - **涉及文件**：测试与脱敏 HTTP/worker 回执，禁止在用户业务库制造样本。
  - _需求: FR-1, FR-3, FR-4, FR-5, FR-6_ ｜ _设计: 测试策略、决策 3_
- [x] 3.3 前端类型/构建、状态测试、真实桌面/375px 截图和原生 UI 观察。
  - **证据块**：项目已有 check/build 脚本、真实 taskdock 数据源；参考原型不当生产证据。
  - **涉及文件**：截图/视觉评审和实际测试日志。
  - _需求: FR-7, FR-8_ ｜ _设计: 决策 4、5_
- [x] 3.4.1 上次源码合同及正常服务目录已核验，并记录实际客户端入口可用性和 NOT_VERIFIED 限制。
  - **证据块**：上次四档完整 schema/annotations 合同、正常 OAuth 元数据回读；tools.listChanged=false、SSE Accept GET 405。
  - **涉及文件**：上次目录/限制回执；不表示客户端缓存或真实调用通过。
  - _需求: FR-1, FR-6_ ｜ _设计: 决策 1、4_
- [ ] 3.4.2 dot 实际 UI 客户端完成认证、目录、read/hash patch/durable exec/原句柄输出闭环，保存脱敏轨迹。
  - **证据块**：本次真实客户端结果待补；脚本 HTTP 或宿主页面不替代。
  - _需求: FR-6_ ｜ _设计: 决策 4_
- [ ] 3.4.3 原有 `@GPTBridge` 实际 UI 客户端通过旧连接器/旧请求闭环与原任务恢复。
  - **证据块**：本次真实旧客户端结果待补；上次旧输入合同回归仅证明源码层。
  - _需求: FR-1, FR-6_ ｜ _设计: 决策 1、4_
- [ ] 3.4.4 实际客户端目录刷新、新会话和断线后原 task/request/job 恢复分别签收，不重复副作用。
  - **证据块**：本次真实平台流程待补，无缓存伪造或强制重启。
  - _需求: FR-4, FR-6_ ｜ _设计: 决策 3、4_

### 阶段 4：已授权提交、本机构建与替换

- [x] 4.1 核 foreign 原字节、精确本轮 diff 和 GitNexus 影响检查，通过后普通 commit/push 并回读远端 SHA。
  - **证据块**：本轮 frozen ownership/foreign.patch；主 Goal 唯一协调者。
  - **涉及文件**：仅本轮 owned 片段，禁止 broad-stage/force-push。
  - _需求: FR-8, NFR-4_ ｜ _设计: 决策 5_
- [x] 4.2 从已推送源码干净导出构建 App，核版本/架构/SHA/codesign，备份旧 App 和安装 manifest。
  - **证据块**：实际安装路径 `/Users/didi/Applications/GPTBridge.app`、Tauri productName/identifier 及活动 job 盘点；基线干净构建脚本由 root 核对。
  - **涉及文件**：本轮制品、构建/备份回执，不迁移凭据或历史。
  - _需求: FR-8_ ｜ _设计: 决策 5_
- [x] 4.3 安全替换并启动，回读安装字节、真实进程、HTTP 与 UI；客户端缺项保留未知。
  - **证据块**：旧 App SHA/回滚目录、新 App 实际签名/进程路径/服务目录/截图。
  - **涉及文件**：安装回执；活动 job 无法安全延续时停止此依赖步骤。
  - _需求: FR-8_ ｜ _设计: 风险评估_
- [x] 4.4 盘点并精确清理本轮可重建缓存，保留当前/回滚制品与证据，报告未验证项。
  - **证据块**：路径、大小、制品身份、活动引用和保留制品 SHA；不泛化清理其他目录。
  - **涉及文件**：清理回执与最终交付报告。
  - _需求: FR-8, NFR-4_ ｜ _设计: 决策 5_

## 上线顺序与回滚

基线冻结 → 说明/元数据/文档/界面 → 相关测试和契约 → 干净提交推送及远端回读 → 同源码 App 构建 → 活动命令核对/旧包备份 → 本机替换 → 实际进程/服务/UI 回读。真实客户端测试可独立开展；缺少可用客户端时不宣称其签收，不混入服务成功。

回滚保留旧 App 和 SHA：安全退出新 GUI，恢复原安装路径旧 App 并读回，保留任务库/原句柄；源码通过本轮审阅反向补丁回退，不 reset 用户工作树。回滚后客户端缓存仍可能需要其支持的刷新。

## 验证与回归

由 root 统一在干净候选目录运行必要命令，子代理不重复构建；实测日志记录命令、源码身份、退出码与覆盖范围：

```sh
.mcp-probe-kit/bin/probe exec check_spec --json '{"feature_name":"dot-compatibility","project_root":"/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal"}'
npm ci
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --test dot_compatibility
cargo test --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path personal-runtime/Cargo.toml
```

相关 Node 状态测试以实际新增文件为准；App 使用本轮已推送干净源码的真实 `desktop:build` 脚本和 `--bundles app`。不把 working tree 中 foreign 的构建清理脚本自动纳入制品。

| 测试组 | 核对点 | 预期证据 |
| --- | --- | --- |
| 目录合同 | 单工作区/网关，各档位名称/schema/annotations/暴露；只允许白名单 | 基线与候选 JSON、严格 diff |
| 旧响应 | server_info 原字段、任务/历史/恢复原请求 | 原回归日志和关键字段对比 |
| 原生闭环 | task_open→read hash→patch→exec→poll/output→checkpoint | 临时工作区真实内容、job、输出与 revision |
| 补丁反例 | STALE_FILE、去重、同 ID 不同输入、并发保留其他改动 | 原回执与冲突/文件最终内容 |
| 恢复/输出 | 提交者退出、死 worker unknown、已完成回执、Unicode/分页/截断 | 原 job 身份与输出证据 |
| 权限/归属 | readonly、档位、路径、缺失/伪造 workspace_id、跨仓 job | 原拒绝码和 read_scope 真实映射 |
| 目录传输 | initialize listChanged=false、SSE GET405、普通GET健康JSON | HTTP 回包与客户端缓存单独结论 |
| UI | 真数据/错误/空/unknown/长路径、键盘、桌面/375px | 本轮真实截图、视觉分及状态测试 |
| 本机交付 | 本轮提交/远端/App源码同源，保留foreign，安装/进程/HTTP/UI | SHA、签名、原路径与运行回读 |
| 客户端 | dot 与原有对话各自发现目录和实际闭环 | 各客户端轨迹；不可用标 NOT_VERIFIED |

## 需求覆盖矩阵

| 需求ID | 设计章节 | 任务编号 | 状态 |
| --- | --- | --- | --- |
| FR-1 | API、决策 1 | 1.1、2.1、3.1、3.2、3.4.3 | 上次源码合同已核；实际旧客户端回归 NOT_VERIFIED |
| FR-2 | 数据模型、决策 1/2 | 2.1、2.2、3.1 | 已完成，见 delivery.md |
| FR-3 | 决策 3 | 2.2、3.2 | 已完成，见 delivery.md |
| FR-4 | 决策 3 | 2.2、3.2 | 已完成，见 delivery.md |
| FR-5 | API、决策 2/3 | 1.1、2.1、2.2、3.1、3.2 | 已完成，见 delivery.md |
| FR-6 | 决策 4 | 1.1、2.1、2.2、3.1、3.2、3.4.1–3.4.4 | 上次服务端通过；实际客户端闭环/刷新/恢复 NOT_VERIFIED |
| FR-7 | 架构、决策 4 | 2.3、3.3 | 本次 0.4.3 原生诊断/回执/输出正常路径与 EOF 已验；完整异常场景组合待验 |
| FR-8 | 决策 5、风险评估 | 1.1、3.3、4.1、4.2、4.3、4.4 | 本次 0.4.3 安装和 19 步 release 写改闭环通过；常驻服务切换及回滚实演待验 |

## 文件变更清单

本轮拥有协议说明精确片段、工作区指南/状态组件及相关测试、上列用户文档/规格、Goal 和 dot-compatibility 证据；最终精确新增/修改列表由 root 按本轮 ownership 和差分出具，foreign 原字节与未跟踪文件不并入。

## 检查点

- [x] 原合同无非白名单漂移，真实只读/网关/恢复反例通过。
- [x] 前端和原生构建成功、真实截图可追溯。
- [x] 本轮提交、远端和安装制品完成独立回读。
- [x] 上次可用入口已评估，缺真实客户端签收的项目明确保留未知。
- [ ] dot 和旧 `@GPTBridge` 实际客户端各自闭环、刷新与断线恢复通过。

## 检查清单

- [x] 数据库、接口/MQ和配置有准确事实或明确“无”。
- [x] 每项任务有源码依据、需求与设计回链。
- [x] 清单默认未完成语义，不伪造 SQL 或客户端通过。
- [x] 根据本轮实际结果更新 Goal 与精确交付报告。

## 上次实际收口与本次继续修复

上次源码 4b2215386b175e97ffff5fb7bf31af6b5a43e6f7 已提交推送，App0.4.2已构建替换并完成真实进程/OAuth/目录/正常原生UI回读。3.4.1 仅表示源码/服务合同与可用性评估；3.4.2–3.4.4 明确未完成，绝不表示真实 dot/@GPTBridge 客户端通过。首次 macOS 后台启动约束失败和系统提示、缓存清理见 [交付记录](../../gptbridge/dot-compatibility/delivery.md)。

本次基于 main `9135ea702b721878457f933420d4f035bb147722` 继续补齐 9 项缺口，实施状态与精确验收条件见 [fixes-20261009.md](../../gptbridge/dot-compatibility/fixes-20261009.md)。本次回执/正常认证诊断/完整输出/只读持久门禁已实现并通过 495 Rust、29 Node、80 Python；0.4.3 构建安装、19 步实际 release 闭环和原生正常界面通过。常驻 MCP 仍为 0.4.2：系统 SQLite 无 sidecars 时只读失败，现有 Homebrew 运行环境可读，但未闭合 RPC 门禁仍阻断。旧签名启动约束、黑屏内部根因、双客户端、异常原生矩阵与回滚实演未完成。
