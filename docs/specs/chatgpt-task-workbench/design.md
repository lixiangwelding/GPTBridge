# 设计与边界

## 概述

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

## 技术方案

### 统一 Windows 应用的 ChatGPT 模式

主进程 `ChatGPT.exe` 可能位于 OpenAI.Codex 安装目录。镜像识别仅区分 unified/independent/unsupported，不因此给 Codex 模式执行资格。unified 分支必须从按钮实际读到 `切换模式，当前模式：ChatGPT`；generic ChatGPT 文本或窗口标题不是模式证据。实机已只读展开菜单，确认按钮、ChatGPT/Codex菜单项和 ExpandCollapse/Invoke 模式，然后收起；未选择模式或发送。

`Confirm-ChatGPTMode` 在 stage 可展开唯一模式按钮、Invoke准确 ChatGPT MenuItem并循环回读；其他动作只核对不切换。发送前若用户改回 Codex则拒绝，仍保留原请求与 UNKNOWN。独立客户端无模式按钮可沿原镜像/模型/插件/输入框验证继续；统一宿主缺按钮一律拒绝。

测试文件只提取纯模式帮助函数，用内存控件替身运行，不加载完整发送器、不查真实窗口。12项模式场景及5项宿主/名称断言覆盖回读、歧义、失败、已开菜单和阶段变更。PowerShell脚本保持UTF-8 BOM。图谱运行不可用时已保留降级，本轮直接核到 Tauri 命令和 WindowsAdapter 两个调用入口；MCP 注册、认证和业务分派不改。

### 对应需求

FR-1～FR-6 沿用独立任务页、原生派发与持久化请求；FR-7～FR-10 对应模式读取、stage切换、send复核与离线控件测试。

### 模型与 Pro 强度

实机模型触发器为“选择 ChatGPT 模型”，模型子项为“选择模型”，可选 GPT-6/GPT-5.6 Sol。强度是自定义 SliderKeyboardControl，不提供 RangeValue。用户确认右端就是 Pro；适配器在前台、准确强度控件和当前聊天模式成立时，先按原生键盘端点/右移调整，再以该控件区域的亮色圆形滑块位置独立回读。没有端点证据不放行。此校验依赖当前控件皮肤与用户确认的档位映射，不是通用OCR或跨版本保证。发送只复核，不调整偏好。

最初候选颠倒的 Pro/深入思考默认值迁移为 GPT-6/Pro，只调整界面偏好，不改批次正文、任务状态或未知发送。

## 文件结构

- `src-tauri/scripts/chatgpt-dispatch.ps1`：宿主、模式、聊天/工作、选项与发送检查。
- `src/lib/chatgpt-tasks/model.ts`、`ddt-plan.json`：偏好迁移与任务草稿。
- `src/routes/chatgpt-tasks/+page.svelte`：运行设置和模式提示。
- `tests/chatgpt-mode-gate.test.ps1`、`tests/chatgpt-tasks.test.mjs`：离线与原生测试。
- `docs/gptbridge/chatgpt-task-workbench/`：20任务、截图、交付回执。

### 结构化 Windows 插件与正文

实机Windows聊天把插件显示为输入框内的原子标记：Text精确为GPTBridge Windows，原始父节点Group带SlotSizedIcon图标，祖父为当前编辑器。单纯提及插件的普通文字、历史消息或Mac标记不算附加。stage可输入@并选择唯一的Windows候选；选中后保留标记，通过编辑器TextPattern把光标移到末尾，再以Unicode输入事件追加正文。每批次核模式、焦点和前台窗口，最后逐字回读并保存requestId/窗口/偏好/提示词SHA/标记前缀。send只读取本次回执并重新核对，不重写内容。Unicode事件构造测试不调用SendInput，不发送Return；真实正文填入仍需独立调度器验收。
