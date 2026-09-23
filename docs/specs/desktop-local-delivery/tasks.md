# 任务清单：desktop-local-delivery

## 概述
按已核代码实现个人插件、本地交付和运行启动；不扩展其他业务仓库。

## 交付物清单
交付类别共 5 类：原生启动、交付元数据、插件包与安装器、测试、版本与说明。文件增减按实际审查清单核验，不修改无关文件。

## 任务列表
- [ ] 1.1 核对真实 Desktop 插件配置与官方安装契约。
  - 证据块：实际 ~/.codex/config.toml 注册项和 codex plugin --help；文件 desktop-plugin/，每个文本文件不超过 200 行。
  - _需求: FR-1_ · _设计: 技术方案_
- [ ] 2.1 新增 profile 选定的原生 stdio/HTTP 启动，不导入或改写配置。
  - 证据块：src-tauri/src/mcp/server.rs:43、mcp/listener.rs:53、lib.rs:personal_cli。
  - 文件：src-tauri/src/desktop_cli.rs，预算 300 行；复用 existing listener/dispatcher。
  - _需求: FR-3_ · _设计: API 设计_
- [ ] 2.2 增加本地绝对交付路径，不把预览/失败/删除算成交付。
  - 证据块：src-tauri/src/tools/dispatch.rs 和 personal.rs。
  - 文件：src-tauri/src/tools/delivery.rs，预算 180 行。
  - _需求: FR-2_ · _设计: 数据模型_
- [ ] 3.1 补专项测试并执行完整回归，核对 stdout、权限与旧任务恢复。
  - 验收点：FR-1、FR-2、FR-3 的正反例，未知结果不重放。
- [ ] 3.2 升版、精确提交推送、安装并检查最新实际监听和其他任务状态。
  - 验收点：配置不丢失、绝对路径回执、安装和运行状态分别记录。

## 检查点
- [ ] 规格与影响面完成。
- [ ] 实现与专项测试通过。
- [ ] 全套回归、代码审查和发布证据完成。

## 需求覆盖矩阵
| 需求 ID | 设计章节 | 任务编号 | 状态 |
|---|---|---|---|
| FR-1 | 技术方案 | 1.1,3.1 | 已完成调查，待实现 |
| FR-2 | 数据模型 | 2.2,3.1 | 待实现 |
| FR-3 | API 设计 | 2.1,3.2 | 待实现 |

## 文件变更清单
| 文件 | 操作 | 行数预算 | 说明 |
|---|---|---|---|
| src-tauri/src/desktop_cli.rs | 新建 | 300 | 启动入口 |
| src-tauri/src/tools/delivery.rs | 新建 | 180 | 交付路径 |
| desktop-plugin/ | 新建 | 每文件200 | 标准插件 |
| scripts/desktop_personal.py | 新建 | 450 | 安装诊断 |
| src-tauri/src/lib.rs、tools/dispatch.rs、tools/personal.rs | 修改 | 原文件少量增量 | 分派接入 |
| 版本清单、专项测试、发布说明 | 修改/新建 | 每文件450 | 兼容验收与交付 |

## 交付前自检
- [ ] 无实现占位符，改动范围与实际 diff 一致。
- [ ] 新模块独立有界，测试与运行证据不混用。
