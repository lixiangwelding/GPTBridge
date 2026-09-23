# 设计文档：high-concurrency
## 概述
对应FR-1、FR-2、FR-3及NFR-1至3。
## 技术方案
### 技术选型
| 类别 | 选择 | 理由 | 关联需求 |
|---|---|---|---|
| 配置 | workspace runtime state/concurrency.json | 不依赖不同客户端环境变量 | FR-1 |
| HTTP | 总量、普通业务、长等待三层许可 | 保留控制容量 | FR-2 |
| stdio | 控制与普通独立有界队列/线程 | 长等待不堵塞输入或取消 | FR-2 |
| 句柄 | 本进程setrlimit | 避免高并发EMFILE | FR-3 |
### 架构设计
Store初始化冻结已验证配置；server、worker与pressure报告复用同一类型。新命令在事务准入前比对磁盘配置，不一致要求安全重启。保守默认兼容已有测试；performance显式持久化到该工作区，不改变OAuth profiles.json。
## 数据模型
ConcurrencyLimits字段包括running、heavy、queued_and_running、http_requests、http_waiting、http_control_reserved、stdio_workers、stdio_control_workers、stdio_queue。无作业schema修改。
## API 设计
server_info增量暴露实际配置、配置文件路径、句柄与重启要求。既有33工具及参数不变。命令结果和pressure使用实际生效限额。
## 文件结构
新增personal-runtime/src/limits.rs和并发专项测试；少量修改store/jobs/worker/locks、listener/stdio、main及server_info。部署脚本提供受保护独立服务重启，旧应用与配置保留回执。
## 设计决策
### 决策1：分层扩容（FR-1、FR-2）
CPU重构建只提高到4；I/O与命令到64。所有数字是许可上限，不承诺相同倍数的真实吞吐。
### 决策2：配置非热切换（FR-1）
不让同一工作区同时执行不同计数的slot池；明确停留旧配置和阻止漂移后的新任务，部署先排空再切换。
## 风险评估
| 风险 | 影响 | 缓解措施 |
|---|---|---|
| 公共Store/HTTP影响面 | 高 | 完整回归+隔离压力测试 |
| 文件句柄和线程增多 | 高 | process句柄和硬范围、状态查询预留 |
| 旧连接重连 | 中 | 预热、空闲复验和保留FRP/任务，非零连接切换承诺 |
