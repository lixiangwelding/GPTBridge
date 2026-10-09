# 需求文档：GPTBridge dot 增量兼容

## 功能概述

在现有 GPTBridge 0.4.2 上说明并展示原生工作区能力，让 dot 在客户端允许的连接器路径下使用同一 MCP。实现本轮已授权的 P0 说明/兼容工作和 P1 宿主指南，验证后由唯一交付协调者 commit/push 并替换本机应用。源码基线为 `17f47d3d261aa23f9e8d2c11d269438e000bceba`，实际结果见仓内 Goal 与 `docs/gptbridge/dot-compatibility/`。

来源：用户附件 `GPTBridge-dot-产品方案与设计稿 (4).html`，2026-10-09。附件的界面状态和拟议 API 是方案资料，不是运行事实；用户当前明确授权实施、提交推送及本机替换，不授予额外云部署、权限迁移或外部消息权限。

## 历史经验与坑

工具输入合同、任务生命周期及共享目录协作已有实现，不能为了直接操作重复造文件工具或执行器。服务目录更新、宿主指南可见、客户端目录刷新和真实调用是不同证据。已有 foreign dirty 必须逐片段保留，只提交本轮拥有的改动；历史截图、模拟原型和浏览器无原生连接预览都不能冒充实际客户端签收。

## 术语定义

原生直接操作指 MCP `tools/call` 分派到现有文件、补丁和命令工具。task/request 是持久身份与幂等记账；job/session/output_ref 是原命令的恢复与输出句柄。dot 和 `@GPTBridge` 是不同客户端使用路径，不是新增服务类型。`policy_only` 表示执行策略与合作调度，`sandbox_enforced=false` 表示没有操作系统级文件系统沙箱。

## 范围边界

In Scope：可选 `server_info.direct_workspace` 说明、INSTRUCTIONS 和六个原有工具 description、文档、契约及恢复回归、工作区同页指南/诊断/任务回执、本机应用交付。

Out of Scope：工具改名或 schema/annotations 改写、认证或暴露分层迁移、目录通知/SSE、新 Direct 执行器、外部 Agent 编排、任务数据格式变化、OS 沙箱、安全策略加固、云部署、自动刷新或计费保证。

## 需求列表

### FR-1: 同一入口与原契约兼容

**优先级:** Must
**用户故事:** 作为原有使用者，我希望更新后继续通过同一连接器使用旧请求。

#### 验收标准（EARS）

1. WHEN 对比基线与候选目录 THEN 系统 SHALL 保持各既有模式/工具档位的端点、认证、工具名、完整 inputSchema、annotations 和暴露规则。
2. WHEN 回放旧请求 THEN 系统 SHALL 保留原响应关键字段和既有副作用，变化白名单仅为审核过的说明文本及可忽略响应元数据。
3. IF schema、权限或旧行为发生非白名单漂移 THEN 交付 SHALL 停止依赖发布步骤并保留差异证据。

### FR-2: 准确表达原生能力

**优先级:** Must
**用户故事:** 作为客户端调用者，我希望确认任务记录与直接文件/命令操作的关系。

#### 验收标准（EARS）

1. WHEN 读取说明与 `server_info.direct_workspace` THEN 系统 SHALL 表达 `native_tools`、`external_agent_required=false` 和原 task/request 持久生命周期。
2. WHEN 读取安全说明 THEN 系统 SHALL 返回实际 strict-read 标志对应的 read_scope，以及 `policy_only` / `sandbox_enforced=false`。
3. IF 客户端能力尚未实测 THEN 系统 SHALL 不返回 `dot_supported=true` 或承诺自动选择连接器、确认豁免与计费结果。

### FR-3: 哈希补丁与任务记账

**优先级:** Must
**用户故事:** 作为并行工作者，我希望修改能保护当前文件并持续保存任务进度。

#### 验收标准（EARS）

1. WHEN 修改已有文件 THEN 调用 SHALL 使用 read_file 的整个文件哈希、原 task_id 和稳定 request_id，继续现有锁与幂等机制。
2. IF 哈希过期 THEN 系统 SHALL 拒绝旧补丁，重读合并后才创建新尝试，不覆盖其他作者改动。
3. WHEN 保存检查点 THEN 系统 SHALL 保留 expected_revision 与请求身份；同 ID 相同载荷返回原回执，同 ID 不同载荷或过期 revision 继续按原规则拒绝。

### FR-4: 持久命令与原句柄结果

**优先级:** Must
**用户故事:** 作为长任务使用者，我希望客户端断线后仍能核对原命令结果。

#### 验收标准（EARS）

1. WHEN 启动非交互命令 THEN 系统 SHALL 复用现有 worker、task/request 和 job 生命周期，按真实 read/build/write 模式协调。
2. WHILE 原 job queued/running THEN 调用 SHALL 有界查询原 session/job，不创建重复副作用。
3. IF 响应丢失或 worker 结果 unknown THEN 调用 SHALL 核对原请求和外部效果，不盲目重放；输出按原引用分页且保留截断与损失标记。
4. WHEN 判断成功 THEN 调用 SHALL 检查真实退出状态、退出码、command_ok 和输出，不以 HTTP 成功或任务 active 代替执行完成。

### FR-5: 权限、路径与网关归属

**优先级:** Must
**用户故事:** 作为多仓库使用者，我希望调用始终落在登记的正确项目。

#### 验收标准（EARS）

1. WHEN 使用网关 THEN 调用 SHALL 先 workspace_list，每次业务工具带已登记 workspace_id，包括恢复与读取输出。
2. IF workspace_id 缺失/伪造、task/job 属于别仓或路径逃逸 THEN 系统 SHALL 继续按原机制拒绝，不能从上一调用或全局 cwd 推测目标。
3. WHEN 查询只读/单工作区/网关安全信息 THEN 系统 SHALL 区分工具档位、命令策略和读取范围，保留单工作区显式外部读取及网关严格读取的原行为。

### FR-6: 服务目录与客户端刷新分别验证

**优先级:** Must
**用户故事:** 作为升级操作者，我希望知道服务和客户端分别看到了什么。

#### 验收标准（EARS）

1. WHEN initialize 或以 SSE Accept 请求 GET THEN 系统 SHALL 保持 tools.listChanged=false 和 HTTP 405；普通 GET 健康 JSON 不算通知流。
2. WHEN 升级服务 THEN 验收 SHALL 重新 initialize/tools/list/server_info，再记录客户端支持的刷新或新会话流程，不能假定自动推送。
3. IF 对应客户端不可用 THEN 验收 SHALL 将 dot 与原有路径各自标为 NOT_VERIFIED，并保留来源和恢复条件，不能用服务端测试替代。

### FR-7: 同页真实指南、诊断与回执

**优先级:** Must（本轮同时实施 P1）
**用户故事:** 作为宿主使用者，我希望同页看清怎样操作及当前真实状态。

#### 验收标准（EARS）

1. WHEN 打开工作区详情 THEN 页面 SHALL 同页提供“dot · 原生工作区”、操作提示、连接诊断和任务/恢复回执，复用既有后端。
2. WHEN 刷新观察或选择任务 THEN 页面 SHALL 只做有界只读查询；复制操作只复制文本，不启动命令、连接器或执行器。
3. IF 加载失败、无任务、长路径、未知状态或客户端信息不足 THEN 页面 SHALL 显示对应真实情况，不补演示成功或把 unknown 映射为已完成。
4. WHEN 桌面/375px、键盘与真实原生应用验收 THEN 页面 SHALL 可用、可读，并留本轮真实截图及视觉评审，原型另标参考。

### FR-8: 可回读的源码和本机交付

**优先级:** Must
**用户故事:** 作为本机使用者，我希望已验收的同一制品替换现有应用并能回退。

#### 验收标准（EARS）

1. WHEN commit/push THEN 协调者 SHALL 只提交本轮改动、普通推送并读回远端 SHA，foreign dirty 保持原字节。
2. WHEN 构建与替换 THEN 协调者 SHALL 从本轮干净源码构建，核 App 版本、架构、SHA、签名、真实安装路径和活动 job，保留旧 App 和回滚 manifest。
3. WHEN 启动替换应用 THEN 验收 SHALL 分别记录安装字节、进程、监听/HTTP、真实 UI 与客户端未知项；命令不能安全延续时停止安装依赖步骤。

## 非功能需求

NFR-1：所有目录、任务、输出读取有界；轮询复用现有生命周期，不引入新的常驻执行器。

NFR-2：日志、截图与文档不得保存 token、OAuth secret、环境变量秘密或未授权路径；回执显示最小必要信息。

NFR-3：身份、配置、任务数据库及恢复句柄兼容；无 schema、配置或数据迁移，不删除任务历史。

NFR-4：单一 Git/安装协调者；子代理权限不扩大，foreign 片段不入本轮候选或提交。

## 依赖关系

已有 Svelte 5 / SvelteKit / Tauri 2 / Rust / SQLite、原 registry/dispatch/personal worker 和 taskdock 查询接口。对应客户端实际可用会话是客户端签收依赖；不可用不阻断独立源码、协议与制品准备，但不能宣称客户端通过。

## 检查清单

- [x] 需求、异常、范围及真实验收层级明确。
- [x] FR-1 至 FR-8 在设计与执行清单逐项覆盖。
- [ ] 对应客户端与本机交付验收按真实证据完成。
