# 任务清单：TaskDock

## 概述

实施已批准设计，完整接入真实数据并升级品牌。维护任务c512be3c-ddb9-4459-a8c4-0b0323c57ea5；不触碰其他会话文档。

## 交付物清单

预计新增约18个模块/组件/测试/文档，修改约8个入口或品牌文件；最终以DELIVERY中的精确文件清单为准。任何超过500行的新增组件拆为表、详情、对话框与状态模块，不复制原型巨型脚本。

## 任务列表

### 阶段1：接口与状态

- [ ] 1.1 实现有界任务快照、计数和搜索，active不冒充执行。
  - **证据块**：personal-runtime/src/tasks.rs:48 `task_list`只返回id/goal/state/revision，无作业状态和时间；src-tauri/src/tools/personal.rs:83 `task_view`已有有界详情。
  - **涉及文件**：personal-runtime/src/workbench.rs及测试各约250行，lib.rs注册1行；专用IPC约250行，拆技能/连接适配。
  - _需求: FR-1, FR-2, FR-5_ ｜ _设计: 数据模型、API设计_
- [ ] 1.2 接入项目与技能真实来源，持久化只降低权限的偏好。
  - **证据块**：src-tauri/src/tools/skill_catalog.rs:14 Catalog已有缓存，manual_only决定自动目录；commands/workspace.rs:20 create_workspace调用原DataStore。
  - **涉及文件**：skill_preferences.rs约150行；skill_catalog.rs局部接入；commands/taskdock_skills.rs约150行。
  - _需求: FR-3, FR-4, FR-5_ ｜ _设计: 决策3_

### 阶段2：界面与品牌

- [ ] 2.1 实现四入口和高密度表格，移除首页配置跳转。
  - **证据块**：src/routes/+layout.svelte:110 onMount将首页跳往workspace配置；现有AppShell仅提供品牌容器。
  - **涉及文件**：layout、首页、projects/skills/settings页面；taskdock子组件和tokens，各控制在300行左右。
  - _需求: FR-1, FR-2, FR-3, FR-4, FR-5_ ｜ _设计: 技术方案_
- [ ] 2.2 更新TaskDock显示品牌并保留旧身份。
  - **证据块**：src-tauri/tauri.conf.json:3 productName与窗口title为旧品牌，identifier为稳定com.lixiangwelding.codingtools.personal。
  - **涉及文件**：package.json、tauri.conf.json、lib.rs托盘文字、README和品牌常量；不移动目录或数据。
  - _需求: FR-6_ ｜ _设计: 决策1_

### 阶段3：验收交付

- [ ] 3.1 逐条执行FR验收并构建可用制品。
  - **证据块**：package.json已有check/build/desktop:build；任务Store为SQLite，Tauri命令注册在src-tauri/src/lib.rs。
  - **涉及文件**：Rust回归、浏览器验收脚本、截图manifest和DELIVERY；证据不混入演示图库。
  - _需求: FR-1, FR-2, FR-3, FR-4, FR-5, FR-6_ ｜ _设计: 测试策略_

## 检查点

- [ ] 规格通过后才能实现。
- [ ] 代码与真实数据接口一致，无模拟成功。
- [ ] 类型、构建、测试、截图、打包逐项记录，不相互替代。

## 需求覆盖矩阵

| 需求ID | 设计章节 | 任务编号 | 状态 |
| --- | --- | --- | --- |
| FR-1 | 数据模型/API | 1.1,2.1,3.1 | 未开始 |
| FR-2 | 决策2 | 1.1,2.1,3.1 | 未开始 |
| FR-3 | API | 1.2,2.1,3.1 | 未开始 |
| FR-4 | 决策3 | 1.2,2.1,3.1 | 未开始 |
| FR-5 | 决策2 | 1.1,1.2,2.1,3.1 | 未开始 |
| FR-6 | 决策1 | 2.2,3.1 | 未开始 |

## 文件变更清单

新增workbench查询、taskdock IPC、技能偏好、页面组件和验收资料；修改现有注册与品牌入口；最终核对DELIVERY的精确路径。

## 检查清单

- [x] 每项任务有真实源码证据与需求回链。
- [x] 单文件大于500行时拆分职责。
- [ ] 完成后精确核对全部交付文件。
