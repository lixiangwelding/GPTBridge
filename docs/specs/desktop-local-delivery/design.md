# 设计文档：desktop-local-delivery

## 概述
对应需求：FR-1、FR-2、FR-3、NFR-1、NFR-2、NFR-3。

## 技术方案
### 技术选型
| 类别 | 选择 | 理由 | 关联需求 |
|---|---|---|---|
| 插件 | 本地 marketplace + 官方 Codex plugin CLI | 不改闭源客户端 | FR-1 |
| 服务 | 原生 profile 选择器、stdio、HTTP listener | 复用真实工具与任务库 | FR-3 |
| 交付 | 统一 delivery 指令和成功补丁绝对路径元数据 | 不新增必须刷新的工具参数 | FR-2 |
| 激活 | 原生健康检查、单实例标识、版本固定制品 | 避免磁盘/进程版本混淆 | FR-3 |

### 架构设计
Desktop 插件通过原生 stdio 调用与 HTTP 相同的 dispatcher。读取指定 profile，不导入或改写配置。HTTP 继续使用保存的 auth、OAuth 密钥和 public URL。启动独立服务不隐式启动额外上游或杀占端口进程。

## 数据模型
工具结果新增 delivery 元数据和已成功文件的 absolute_path；原字段不删改。部署回执保存本机路径、版本、SHA、服务状态，私密材料不入库。

## API 设计
| 方法/函数 | 路径/签名 | 入参/出参 | 关联需求 |
|---|---|---|---|
| CLI | --personal-stdio PROFILE | JSON-RPC stdin/stdout | FR-1 |
| CLI | --personal-serve PROFILE PORT | 本地鉴权 HTTP | FR-3 |
| delivery | 成功 apply_patch | 本地绝对文件路径 | FR-2 |

## 文件结构
新增 src-tauri/src/desktop_cli.rs、src-tauri/src/tools/delivery.rs、desktop-plugin/、scripts/desktop_personal.py 和专项测试。修改 lib.rs 启动分派、dispatch.rs/personal.rs 交付指令，以及既有五份版本清单。

## 设计决策
### 决策 1: 不从服务端伪造客户端可见状态（关联需求: FR-1）
问题：server_info 不能证明输入框已渲染插件。选项：篡改缓存或安装标准插件。决策：采用官方安装接口，并回读插件目录。
### 决策 2: 不强停并发任务（关联需求: FR-3）
问题：现有旧 GUI 没有无界面恢复入口。决策：先验证新服务，再检查切换条件；不满足时保持现有服务并明确未完成项。不得终止他人 worker、清理其作业或重放命令。

## 风险评估
| 风险 | 影响 | 缓解措施 |
|---|---|---|
| 启动入口影响公共工具 | 高 | 隔离配置、完整协议和回归测试 |
| 监听切换瞬时连接 | 高 | 预热验证、空闲检查、保留任务与回滚回执，不声称旧架构支持无损热切换 |
| 插件缓存未刷新 | 中 | 官方安装/列表验证，与界面验收分开 |
