# 设计与边界

- Svelte 独立路由：左侧依赖任务队列、中间提示词和回执、右侧桌面/网页派发设置。沿用现有 Shell，增加一个导航项。
- `src/lib/chatgpt-tasks/model.ts` 定义导入校验、依赖 DAG、状态转换及完整提示词。`ddt-plan.json` 是三条正式交接的 20 个草稿，不是历史任务已恢复执行的证明。
- 本地草稿保存在此界面专用 localStorage key，可导出备份；不修改工作区配置或 ChatGPT 权限。原始任务输入文件不自动读取或上传。
- `chatgpt_task_dispatch` 是 Tauri 桌面 IPC，不暴露到 MCP。Windows 启动自有 PowerShell 子进程，参数只传 JSON 路径，不将提示词拼进命令。
- 适配器只操作唯一 ChatGPT 目标窗口。网页还要核 chatgpt.com 地址栏。准确模型按钮、思考选项、已附加且可移除的 Windows 插件 chip、输入框 ValuePattern 均须匹配；缺失则拒绝，不使用坐标或盲打。
- stage 逐字回读后生成请求与窗口、模型、强度和提示词绑定。send 原子创建不可重放记录；超时、崩溃、按钮调用后无确定回读均保留 UNKNOWN。不能从桌面端未知状态自动回落网页。
- 使用现有 Windows app_config_dir 下独立 chatgpt-task-dispatch 目录。仅自有子进程 kill_on_drop；现有 MCP、FRP 和 ChatGPT 进程不重启。
- UIAutomation 控件名称、可访问性模式在真实客户端尚未核实，Pro 与思考强度组合也未验证。适配器拒绝是预期缺口，不能改为绕过检查。Chrome/Edge 网页 fallback 不覆盖其他浏览器或 PWA。

图谱：已尝试项目要求的 impact；托管 GitNexus 安装因 pnpm 参数错误失败。返回降级，不含可靠调用图。实际源码核对显示 Shell 由根 layout 引用；本次仅加导航项。Rust 入口仅增加命令注册，不修改现有 MCP 调度函数。原有脏文件保留。

## 独立调度补充

用户明确批准本地队列交接例外。`scripts/chatgpt-tasks/submit_queue.py` 只校验/提交原子文件或读状态；`scheduler.py` 由 Windows 所有者在原生界面启用，使用现有 WSL Python。独立 owner 锁避免多消费者；批次/请求持久化、nonce、产物与日志 SHA、依赖推进由同一 Engine 实现，离线测试使用内存适配器，不启动任何 ChatGPT 进程。`chatgpt_queue_control` 为原生桌面控制入口，不注册 MCP。独立调度已启用时手动发送拒绝，防止同窗口争用。
