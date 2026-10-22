# ChatGPT 任务工作台 · 独立自动调度

2026-10-22。GPTBridge 新增 `/chatgpt-tasks` 独立页面。三条正式交接已整理成 20 个可执行任务与依赖。Codex 只提交文件队列，由用户独立启用的 Windows 调度器控制 ChatGPT，只有 ChatGPT 调用 GPTBridge Windows；Mac 不调用、不修改。

## Windows 统一应用的 ChatGPT 模式

用户已确认使用当前 Windows 应用左上角的 ChatGPT 模式，而不是另外安装客户端。`OpenAI.Codex` 包内的 `ChatGPT.exe` 同时承载两个模式，不能按包名整体拒绝。只有用户独立启用的 Windows 调度器可在任务准备阶段选择“ChatGPT 创建、学习和探索”，回读模式及“聊天”On/“工作”Off；inspect/send只核对、不切换。Codex 不直接调用 GPTBridge 或执行发送步骤。本聊天新增的 Windows 任务页 Computer Use 控制授权见下方；其他聊天仍按原边界。

实机只读核实了模式菜单、模型菜单、输入框“询问 ChatGPT”和输入框内带图标的结构化 GPTBridge Windows 标记。Pro 是“强度”滑条的右端，已由用户明确确认，不是模型名。默认 GPT-6/Pro；旧候选颠倒的 Pro/深入思考偏好会迁移，任务正文、依赖和状态保持不变。

模型使用可访问性选中状态回读；当前自定义强度条不提供 RangeValue，因此以唯一控件、前台窗口、控件区域圆形滑块端点核对 Pro。控件皮肤变更或无法确认时拒绝发送。结构化插件标记需保留，不能把普通聊天文字当附加；发送前再核完整正文和本请求回执。该实现仍需用户独立启用后的真实填入/发送验收。

## 预期使用方式

一次在 Windows GPTBridge 中选定项目、桌面客户端/网页、模型与思考强度，启用独立调度器。以后在 Codex 中交代工作，由 Codex 生成任务包和完整提示词，提交本地队列；调度器自动建新聊天、核对选项和 Windows 插件、填入并发送，等待本次结果回执后推进依赖任务。Codex 只读取本地状态和产物，不运行发送器。

每个任务的结果 JSON 带 batchId、taskId、本次随机 nonce、实际产物 SHA、验证命令、exitCode 和日志 SHA。调度器独立读取文件与 SHA；失败/阻塞/未知不升级成完成，不执行回执中的命令。此核对证明回执及其产物身份，不独立保证每项业务验收内容真实完整；未验运行、游戏、奖励与生产层级必须继续保留。

桌面为默认目标。只有准备阶段明确尚未发送时，才允许按勾选设置回落网页；一旦发送尝试发生，重启、超时和异常均保留原请求，不重发。暂停可保留已填入但未发送的同一请求；停止不关闭用户 ChatGPT、其他聊天或已有 MCP。调度器不触碰 Mac。

## Windows 运行环境与当前状态

交付是 Windows `.exe` 和 Windows 桌面界面；源码在 WSL 维护，GitHub `windows-latest` 编译。当前队列引擎使用本机已有 WSL Python，尚非脱离 WSL 的 Windows 原生独立实现；不在本机安装 Rust，Mac 不动。

- 已安装源码 `955a22306bbfd3cfe52ecc170c3c710e20c76c85`，Windows [构建 39783260329](https://github.com/lixiangwelding/GPTBridge/actions/runs/39783260329) 成功：21 Node、19 Python、6 Rust 定向测试通过；全量 Rust 测试未执行。Windows 连接服务和三个本地 MCP 正在运行。
- 原生技能目录已显示 477 项，扫描截断提示消失；修复 Windows 嵌套与共享正文引用。详情预览最后一次自动化动作报错，尚未取得原生详情通过证据。[本地 MCP 与技能说明](windows-local-mcp-skills.md) 记录具体边界。
- 官方 Windows Computer Use 曾实际读取并操作任务页，WSL 中指向 Mac 的错误启动项已修正。用户随后要求停止 Computer Use，本次会话已关闭，不再操作窗口；当前聊天工具目录未热加载，也没有重启活动聊天。
- 本聊天此前的 UI 授权仅覆盖 Windows GPTBridge；直接 Bridge MCP/HTTP、发送脚本、直接发送按钮和 Codex 操作 ChatGPT UI 仍禁用，Mac 不动。后续后台检查不绕过该边界。
- 最后一次准备诊断已识别一个 ChatGPT 模式窗口，因发现“停止”按钮而等待；原 T01 请求未发送。技能候选安装前已停止调度器，当前仍为 stopped，原 20 项任务与请求保留，发送标记和回执均为 0。
- Windows 控制与调度阶段日志已安装并有真实启用/继续/停止、窗口筛选和 busy 事件。[Windows 诊断说明](windows-diagnostics.md) 提供位置及排错方式。

[交付回执](delivery.json) 给出安装和运行身份。[历史任务页截图](windows-native.png) 拍摄于后继日志与技能修复之前，不能代表当前技能目录或队列状态。完整输入、发送、GPTBridge 业务执行和 20 项任务完成仍未验收。

## Codex 可以使用的本地入口

提交和读状态的 CLI 不调用 GPTBridge、ChatGPT，不启动调度进程。以下为已登记的 WSL 工作区与 Windows 队列，提交同 batchId 同内容为幂等回读，不创建第二批：

```bash
python3 scripts/chatgpt-tasks/submit_queue.py submit \
  --plan docs/gptbridge/chatgpt-task-workbench/tasks.json \
  --prompts docs/gptbridge/chatgpt-task-workbench/prompts \
  --workspace /home/lixiang/my-project-java/codeVerifyRe0 \
  --queue /mnt/c/Users/99321/AppData/Roaming/coding-tools-mcp-personal/chatgpt-task-queue \
  --batch-id ddt-three-lines-20261022-01a16250

python3 scripts/chatgpt-tasks/submit_queue.py status \
  --queue /mnt/c/Users/99321/AppData/Roaming/coding-tools-mcp-personal/chatgpt-task-queue
```

批次文件发布采用原子不可覆盖硬链接，输入开始执行后绑定 SHA，不修改既有批次。任务结果在工作区 `output/gptbridge-chatgpt-results/<batchId>/`。旧 paused Goal 不自动恢复；本批的权限与范围写在提示词中，不连接生产、不部署、不提交推送。独立调度器需要 Windows 当前已有 WSL Python，不在 Windows 安装 Rust 或 Python。

## 验收与恢复边界

任务 UI 保存/导出不等于队列提交；提交不等于派发；消息出现不等于任务完成；回执/产物哈希通过不等于原端、真实游戏或奖励到账。结果 UNKNOWN 必须先查询原任务回执和对话，不删除 send-attempt 文件、状态或 owner 标记以制造重试资格。原生启动失败导致残留 owner 标记时先核对 WSL 进程和原请求，再人工处理，当前不做猜测式自动清理。

## 20 个任务与完整提示词

- [T01 冻结三线接续基线](prompts/T01.md) · 六版 · 依赖 无
- [T02 修复 Windows 工作台启动](prompts/T02.md) · 工作台 · 依赖 T01
- [T03 固化 Windows 可重建交付](prompts/T03.md) · 工作台 · 依赖 T02
- [T04 接入 V2 最终联合候选](prompts/T04.md) · V2 · 依赖 T01, T02
- [T05 核验 V2 原端装载身份](prompts/T05.md) · V2 · 依赖 T04
- [T06 处理 V2 已知回调风险](prompts/T06.md) · V2 · 依赖 T04
- [T07 核对 V2 特殊入口映射](prompts/T07.md) · V2 · 依赖 T04
- [T08 补查 V2 迷宫与 Crypt](prompts/T08.md) · V2 · 依赖 T04
- [T09 核验 V2 资源消费者](prompts/T09.md) · V2 · 依赖 T04
- [T10 建立 V2 逐关验收批次](prompts/T10.md) · V2 · 依赖 T05, T06, T07, T08, T09
- [T11 核验 V7 准确 CLR 装载](prompts/T11.md) · V7 · 依赖 T01, T02
- [T12 补齐 V7 资源缺口](prompts/T12.md) · V7 · 依赖 T01
- [T13 核验 V7 小关与四档](prompts/T13.md) · V7 · 依赖 T11, T12
- [T14 闭合 V7 VIP 与费用](prompts/T14.md) · V7 · 依赖 T11, T13
- [T15 闭合 V7 奖励到账](prompts/T15.md) · V7 · 依赖 T11, T13, T14
- [T16 执行 V7 首批完整本](prompts/T16.md) · V7 · 依赖 T13, T14, T15
- [T17 补 V1 与 V4 调试接线](prompts/T17.md) · V1 / V4 · 依赖 T01, T02
- [T18 补 V8 与 V9 调试接线](prompts/T18.md) · V8 / V9 · 依赖 T01, T02
- [T19 补齐签到原图与数据对照](prompts/T19.md) · V7 / 工作台 · 依赖 T02
- [T20 六版整合与交接](prompts/T20.md) · 六版 · 依赖 T03, T10, T16, T17, T18, T19
