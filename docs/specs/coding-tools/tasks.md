# 实施与验收任务

| ID | 实施对象及验收 | 对应需求 |
|---|---|---|
| P01 | 独立数据根和个人标识，原服务配置不变 | FR-6 |
| P02 | 持久任务库与多任务绑定，步骤修订 CAS | FR-1/3 |
| P03 | SHA 补丁、跨进程短锁、幂等回执 | FR-2/3 |
| P04 | 独立命令 worker、并发槽位、终态与分页 | FR-4/5 |
| P05 | MCP/Actions 共用集成和简化会话入口 | FR-1/2/3/4 |
| P06 | 配置继承、隔离启动说明和脚本 | FR-6 |
| P07 | 自测、故障注入、回归与个人仓库推送 | FR-7 |

证据入口：src-tauri/src/tools/{context,dispatch,patch,exec,session,registry,file}.rs、src-tauri/src/mcp/server.rs、src-tauri/src/data/migrate.rs、src-tauri/src/platform/{macos,linux,windows}/mod.rs、src/lib/components/ChatGptSessionPrompt.svelte。

新功能模块按 store/tasks/locks/jobs/worker 拆分，每个模块尽量小于 500 行；已有大文件只增量接入。验收不得用运行启动替代成功终态，不得用模块测试替代桌面或真实账号联调。
