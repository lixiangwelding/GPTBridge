# 0.3.6：Desktop 插件、本地交付与独立服务启动

日期：2026-09-23。基线：个人仓库 main / 73e038e494b07bbe9d35414cd4aae9bc7f417416。

## 本版修复

| 项目 | 实现 |
|---|---|
| Desktop 插件身份 | 新增标准个人 marketplace 插件包，显示名 codex无限，稳定 ID codex-infinite；默认映射用户已有 asdk_app_6a848a4ede608191af51370794d3091d，不创建新的公网授权，也不加载重复 MCP |
| 本地文档交付 | initialize/task_open 明确默认写入本地工作区、返回绝对路径；优先已有项目输出路由，无路由时建议 docs/deliverables |
| 成功文件回执 | apply_patch 成功结果增加 local_delivery，含已确认存在且在工作区内的绝对路径及写入回执哈希；预览、错误、删除、缺失或越界文件不冒充交付 |
| 无 GUI 启动 | --personal-stdio PROFILE_ID 和 --personal-serve PROFILE_ID PORT 复用真实 dispatcher、保存的工作区/权限与同一任务库，不启动 GUI 或修改配置，不抢占占用端口 |
| 原生 stdio | JSON-RPC 单帧限制 2MiB、4 个工作线程、队列 32；stdout 只输出协议 JSON，缺 ID 的工具调用不作为通知执行 |
| HTTP 与任务恢复 | 保留原 OAuth/Bearer 配置、公共地址和持久 task/job；旧 request_id 与补丁哈希保护不变 |
| 插件安装 | scripts/desktop_personal.py 使用官方 Codex plugin CLI，备份客户端配置并核验其他配置；默认 registered，显式 --transport stdio 才切到本机进程，不同时加载两套工具 |
| 受控启用 | scripts/service_personal.py 先预热同一制品，核对进程身份、其他活跃/排队作业和 RPC 完成记录，再执行显式空闲 GUI 交接；提供用户级 LaunchAgent、失败恢复及真实版本回读 |

## 不打断其他任务的边界

不关闭 ChatGPT Desktop，不改变辅助访问权限，不终止其他 worker，不删除或重放历史任务，不强行解锁。已有源码协作锁仍会让构建和写入正常排队。

初次从旧 GUI 内嵌监听迁到独立服务不是零连接切换：经两次空闲核验后，空闲 HTTP keep-alive 连接可能重新建立。不能把这一点描述为无损热迁移。其他作业或在途 RPC 不为空时保持旧服务；最终安装、启动与实际远程连接状态分别记录。

## 插件可见性

本机确认的客户端为 /Applications/ChatGPT.app，版本 26.915.31945。插件通过受支持的本地 marketplace 登记，默认使用 .app.json 指向原连接。本地安装、配置 enabled 和插件目录已识别是可回读证据；当前打开的输入框是否已刷新需要独立 UI 验收，不因安装成功就宣称已观察到 @ 菜单。

不为刷新目录重启正在使用的 Desktop。平台说明： https://developers.openai.com/plugins/build/plugins 。本版不修改闭源客户端或账号安装策略，不把本地注册等同公开目录发布。

## 命令入口

插件安装器需要 Python 3.11+；当前 Mac 已有 /opt/homebrew/bin/python3。

```bash
/opt/homebrew/bin/python3 scripts/desktop_personal.py install --profile PROFILE_ID
python3 scripts/service_personal.py --profile PROFILE_ID --task-id TASK_UUID
```

service_personal 默认不停止旧监听。初次 GUI 交接还需显式 --handover-pid，并通过真实 PID、制品、空闲作业和 RPC 核验。仅操作确认的个人应用主进程；不向进程组发信号，不强杀其他作业。启动与安装回执保存在本仓库 .artifacts/desktop036。

## 验证

新增 21 个测试：原生协议 3、文件交付 4、实际原生 stdio/HTTP 集成 3、插件包 4、服务保护与 RPC 闲置识别 7。

最终全量回归：.artifacts/selftest/1790138247911990000/result.json，14 阶段均通过。覆盖 Python、任务运行时、worker 构建、MCP 合同/协议、权限、工具目录、Skill、全部 Rust 回归、前端测试/检查/构建和桌面构建。

源码指纹：9cbe4f591aea366815ca2ed3cec26079d0e87fd8921f7770cf4b1c0178f3359b。

Mac app 打包：.artifacts/checks/bundle-036-1790138056631269000/result.json，通过。打包后只加强了独立 Python 交接检查，没有变更二进制对应 Rust、前端或版本清单；最终回归覆盖最新脚本。

集成测试实际证明：新 CLI 能初始化并返回 33 个真实工具、输出不污染 JSON-RPC、配置字节不变、无凭据 HTTP 调用被拒绝、正确 Bearer 可用、第二监听不能抢占第一个监听、未知 profile/非法端口不会落入 GUI。

首次集成失败是测试夹具漏填必需 tunnel 字段并误用 auth_type 而非保存格式中的 type。已按实际模型修正夹具并重新完整回归，不放宽生产配置解析。

未声称完成 Windows/Linux 实机制品、当前输入框展开截图或所有外部客户端缓存刷新。安装与连接验收以各自最终回执为准。
