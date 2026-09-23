# 任务清单：high-concurrency
## 概述
扩容使用同一配置源，不更改业务项目。
## 交付物清单
统一配置模块、传输/执行扩容、压力与过载测试、安装启用回执、发布说明共五类，实际文件以精确git diff为准。
## 任务列表
- [ ] 1.1 新建limits模块并在Store加载，约250行；验证配置和默认兼容。
  - 证据块：personal-runtime/src/store.rs:47和jobs.rs:9。
  - _需求: FR-1_ · _设计: 数据模型_
- [ ] 1.2 接入worker实际限额、保留原slot锁语义并补运行上限测试。
  - 证据块：personal-runtime/src/worker.rs:84和locks.rs:29。
  - _需求: FR-1, FR-2_ · _设计: 技术方案_
- [ ] 2.1 接入HTTP三层许可及stdio控制队列，句柄仅改本进程。
  - 证据块：src-tauri/src/mcp/listener.rs:238、desktop_cli.rs:112和main.rs。
  - _需求: FR-2_ · _设计: 架构设计_
- [ ] 3.1 验证96任务高并发、256入队上限、控制请求在饱和中成功，重跑全部回归。
  - 证据块：personal-runtime/tests/concurrency_recovery.rs、src-tauri/tests/desktop_native.rs。
  - _需求: FR-3_ · _设计: 风险评估_
- [ ] 3.2 升版提交安装，配置performance，安全切换指定个人服务并实际读取生效值。
  - 证据块：scripts/upgrade_personal.py、scripts/service_personal.py。
  - _需求: FR-3_ · _设计: 文件结构_
## 检查点
规格通过后实现；测试通过后发布；实际生效值回读后声明开启。
## 需求覆盖矩阵
| 需求ID | 设计章节 | 任务编号 | 状态 |
|---|---|---|---|
| FR-1 | 数据模型 | 1.1,1.2 | 实施前 |
| FR-2 | 技术方案 | 1.2,2.1 | 实施前 |
| FR-3 | 文件结构 | 3.1,3.2 | 实施前 |
## 文件变更清单
配置、传输、执行、压力专项、发布脚本和说明。新增模块不超过500行，已有大文件仅局部变更。
## 交付前自检
所有声明均附测试/运行回执；不删其他会话文件、不重放未知结果、不强行解锁。
