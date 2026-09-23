# 设计文档：TaskDock

## 概述

覆盖FR-1至FR-6及NFR-1至NFR-3。沿用已批准的石墨绿导航、暖灰底与紧凑任务列表；取消演示任务、虚假连接与全局权限开关。

## 技术方案

### 技术选型

Svelte5组件和现有Lucide图标；Tauri专用IPC；同一SQLite Store，不另设任务数据库。技能偏好单独持久化，只降低自动可发现范围。

### 架构设计

TaskDock页面 → 类型化API → 专用Tauri命令 → 已保存项目 → 原Store/Skill Catalog。
原配置页面保持原路由，放在设置的高级入口。只读任务查询按项目去重，分页排序，并报告不可读项目，禁止悄悄当空库。

## 数据模型

TaskRow包含workspace_id、task_id、goal、state、display_state、updated、revision、running_jobs、queued_jobs和摘要。显示态独立于持久任务态：running/queued/attention/done/ready/paused。真实运行作业优先，已完成但未验证归入待处理。
Snapshot包含tasks、counts、matched_total、cursor、observed_at和warnings。任务详情包含有限步骤、作业和事件；全文只在打开详情时读取。
技能自动匹配偏好只记录已停用skill_id与版本；默认不覆盖源文件manual_only。

## API 设计

| 命令 | 入参 | 出参 | 需求 |
| --- | --- | --- | --- |
| taskdock_snapshot | 项目、搜索、状态、游标、页长 | 有界任务页与计数 | FR-1 |
| taskdock_task | 项目、task_id | 任务详情、作业摘要 | FR-1、FR-5 |
| taskdock_create | 项目、目标、request_id | 真实任务回执 | FR-2 |
| taskdock_job_output | 项目、task_id、job_id、流、偏移 | 归属校验后的有限文本 | FR-5 |
| taskdock_skills | 项目、查询、游标 | 真实目录与偏好 | FR-4 |
| taskdock_skill_read | 项目、skill_id、偏移、SHA | 有界技能正文 | FR-4 |
| taskdock_skill_preference | 项目、skill_id、启用、预期版本 | 持久化回执 | FR-4 |
| taskdock_connections | 项目 | 本实例状态及端口探测证据 | FR-3 |

## 文件结构

personal-runtime/src/workbench.rs提供只读查询和数据测试；src-tauri/src/commands/taskdock.rs与相关窄模块负责IPC；src/lib/taskdock保存类型、状态、样式和组件；新路由为首页、projects、skills、settings。已有workspace详情和设置子页保留。

## 设计决策

### 决策1：显示品牌改名，身份兼容（FR-6）
保留bundle identifier、数据路径、协议工具名和服务脚本入口。改变这些会导致用户“配置丢失”或双实例冲突，品牌升级不等于身份迁移。

### 决策2：任务目标不伪装为执行（FR-2、FR-5）
创建后返回接续指令，实际执行仍由AI客户端及既有工具驱动。未知作业不能在UI任意重放；确认能力不足时明确说明，不制造只在前端生效的安全按钮。

### 决策3：并发读与局部写（FR-1、FR-4）
SQLite读事务为每项目页与计数提供一致性；多项目时间戳分别标识。技能偏好做乐观版本校验；不改调度器和资源锁实现。

## 测试策略

Store测试覆盖状态优先级、游标、搜索转义、同快照计数、1000任务、有界输出和不改写。IPC测试覆盖项目目录验证、跨任务拒绝、技能偏好和源限制。Svelte类型检查和生产构建；真实构建浏览器覆盖四新页与旧路由兼容、手机抽屉、空态、错误与长目标；原生Tauri编译和打包独立报告。

## 风险评估

高风险：权限、重启和任务写入。通过窄IPC和原接口降低影响，不实现任意调用桥。中风险：全局壳层与导航；保留原路由并补回归。GitNexus当前回退没有调用图，采用显式源码引用和测试补充，不声称图谱完整。

## 检查清单

- [x] 覆盖所有FR和兼容要求。
- [x] 数据模型、接口及验收边界明确。
