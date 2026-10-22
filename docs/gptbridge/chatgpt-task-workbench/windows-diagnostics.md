# Windows 实机与本地诊断

交付目标是 Windows GPTBridge 安装包和 Windows ChatGPT 客户端。源码在 WSL 维护，候选由 GitHub `windows-latest` 编译为 `.exe`。当前独立队列引擎仍使用本机已有 WSL Python；这项依赖不是 Linux 客户端，也不等于 Windows 原生独立运行。Mac 不访问、不修改。

## Computer Use 修复

WSL 的 `node_repl` 配置原先指向不存在的 Mac 应用路径。已改为启动本机已安装的官方 Windows `node_repl`，使用官方 `@oai/sky`。仅修改这一配置段，保留私密备份，不重启活动聊天或 WSL。当前聊天的工具目录未热加载；隔离的官方 MCP 会话已实际完成 Windows 窗口枚举和任务页读取。

首次仅文字观察后的点击返回 `coordinate input geometry is unavailable`。重新绑定官方返回的唯一窗口、激活并取得截图状态后，启用调度器按钮点击成功；真实队列心跳首次出现 PID 79668，后继日志版 PID 85034，T01 沿用原 requestId/nonce 恢复。该证据证明 Windows 原生读取和按钮输入，不证明任务消息已发送。

用户已明确允许本聊天通过 Computer Use 操作 Windows GPTBridge 任务页的启用、继续、暂停和停止控件。本机硬拦截按聊天 ID、单个调用代码 SHA 和有效期授予窄范围例外。直接 GPTBridge MCP/HTTP、直接发送按钮、发送脚本和 Codex 操作 ChatGPT UI 仍禁止；Mac 不动。单个调用结束后撤销临时授权文件。用户随后明确要求停止 Computer Use，本次官方会话已关闭；后续仅进行后台文件、日志和 Windows 系统元数据检查，不再操作窗口。

## Windows 应用日志

日志版候选在 Windows 用户配置目录的 `coding-tools-mcp-personal\chatgpt-task-queue\logs\` 写入：

- `windows-controls.jsonl`：启用、继续、暂停、停止和提交的执行结果及耗时；状态轮询不刷屏。达到 2 MiB 后保留一份 previous 文件。
- `scheduler-events.jsonl`：调度器启动/状态变化/退出、恢复原准备请求、适配器开始/结果/异常、发送尝试/结果、回执校验。包含任务和请求关联、耗时、退出码、错误类型、标准错误长度与 SHA；达到 2 MiB 后轮转，最多三份历史。
- 准备结果的 `diagnostics`：已识别宿主、当前模式、目标窗口数量、被离屏条件排除的数量、根控件类型、发现异常类型、触发忙碌的按钮数量和固定名称；继续保留桌面与网页各自的准备审计。

日志不序列化完整请求、提示词、剪贴板或凭据；不保存标准输出/标准错误正文。日志写入失败不能使已发送请求变成可重放。原有发送前持久化 UNKNOWN、任何发送标记禁止恢复、原请求不重发的校验保持生效。

Computer Use 自身的启动、权限表单、单元代码 SHA、耗时和错误保存在本机私密目录 `~/.local/share/codex-computer-use-repair/`；配置备份和这些操作日志不提交到 GitHub。

## 当前证据与未验证项

安装、启动、客户端准备、消息发送、GPTBridge 业务执行及 20 个任务完成分别验收。最新现场、安装 SHA 和 Windows CI 结果见 [交付回执](delivery.json)；截图 `windows-native.png` 记录日志版之前的任务页，不能作为后继队列状态或日志安装证明。

出现 busy 时保留原请求等待；无法唯一核对目标窗口、模式、模型、Pro 或结构化插件时停止准备。仅准备明确 blocked 且无发送标记/业务回执时，继续按钮才允许沿用原请求恢复。不能删除审计或重交批次制造重试资格。

## 最终后台回读

最新技能候选已安装，源码 `955a22306bbfd3cfe52ecc170c3c710e20c76c85`。真实控制日志包含启用、继续、停止；准备诊断读到 hostKind=unified、mode=ChatGPT、targetCount=1、busyButtonCount=1、busyButtonNames=[停止]，无发现异常。该诊断来自已停止的日志版 worker 85034；后继安装未重新发送任务。

本机 WSL 可直接读取日志：

```text
/mnt/c/Users/99321/AppData/Roaming/coding-tools-mcp-personal/chatgpt-task-queue/logs/windows-controls.jsonl
/mnt/c/Users/99321/AppData/Roaming/coding-tools-mcp-personal/chatgpt-task-queue/logs/scheduler-events.jsonl
```

先看调度器心跳，再看最近的状态变化和 adapter_result。当前心跳为 stopped；批次内历史 running 不表示后台仍在执行。不要删除原审计或 send 标记来强行继续。安装、MCP 服务运行、477 项技能目录、消息派发及任务完成的证据见 [delivery.json](delivery.json)，分别报告。
