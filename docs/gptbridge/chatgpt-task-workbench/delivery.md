# Windows GPTBridge 修复与安装交付

2026-10-22。最新 Windows 候选 `955a22306bbfd3cfe52ecc170c3c710e20c76c85` 已由 GitHub 编译并安装到本机。后台核对安装 EXE、进程、监听与本地 MCP 子进程身份；用户要求停止 Computer Use 后，已关闭本次官方自动化会话，不再操作窗口。Mac 未访问、未修改。

## 已交付

- 新建 ChatGPT 任务页，保留原 20 项任务包与完整提示词；修复准备检查顺序、已证实未发送请求的继续恢复，以及 Svelte Proxy 保存异常。
- 增加 Windows 控制日志和调度器阶段日志，记录状态、耗时、错误、窗口识别、模式和忙碌原因，限制大小并轮转，不记录提示词和凭据正文。
- Windows 本地接入 Playwright、jshook、drawio，独立原生 stdio 握手与目录发现分别返回 25、8、8 个工具。配置应用后，三个 Windows Node 服务各有一个运行进程；未执行业务工具。
- 修复 Windows 嵌套技能与共享参考文件的路径引用，将目录扫描上限从 4,000 改为有界的 8,192。已安装原生页面显示 **477 项技能、52 个内置工具**，扫描截断提示消失。最后一次详情预览自动化动作报错，随后用户停止 Computer Use；因此不把详情预览或所有技能业务执行写成已通过。

## Windows 验证

[Windows 构建 39783260329](https://github.com/lixiangwelding/GPTBridge/actions/runs/39783260329) 成功：21 项 Node、19 项 Python、6 项 Rust 定向回归实际通过，Svelte 0 错误/0 警告，PowerShell 语法、模式/Pro/插件标记夹具及 NSIS 打包通过。6 项 Rust 包含 3 项工作区映射和 3 项 Windows 技能路径/扫描边界测试；全量 Rust 测试未执行。

安装退出码 0，Windows EXE SHA-256 为 `f4966d70f926fb2977562304a3df5639ec0e7bd7327aec2e94a1dc9d2ced8538`。后台回读 PID 35512，监听 `127.0.0.1:38766`。本地配置只增加这三项上游 MCP，其他字段逐项等价，67 个其他任务文件 SHA 保持不变。完整制品、安装与运行身份见 [JSON 回执](delivery.json)。

## 原队列的实际状态

原批次 `ddt-three-lines-20261022-01a16250` 与 T01 原 requestId/nonce 保留。日志版曾通过 Windows 原生控制启动、继续并恢复原请求，识别到唯一的 ChatGPT 模式窗口和一个“停止”按钮，因此等待客户端，没有填入或发送。升级最终技能修复版前已停止调度器；当前仍为 **stopped**，发送标记 0、结果回执 0，20 个任务尚未执行完成。批次文件保留的 running 是历史执行状态，不能代替调度器心跳。

Codex 不通过后台脚本启动发送器或绕过独立启用边界。用户停止 Computer Use 后，当前工作只进行文件、配置、日志、Windows 系统元数据和交付检查；Windows GPTBridge 连接服务继续运行。实际消息发送、ChatGPT 调用 Bridge 工具及任务业务结果尚未验收。

## 日志与说明

- [Windows 本地 MCP 与技能修复](windows-local-mcp-skills.md)
- [Windows 诊断与日志入口](windows-diagnostics.md)
- [20 项任务与提示词入口](README.md)

当前队列引擎仍依赖本机已有 WSL Python，窗口及三个 MCP 服务器运行在 Windows。`windows-native.png` 是 `d48dc67` 时的历史任务页截图，不能作为当前技能数量或队列状态的证据；前端保存与手机截图证据的拍摄版本保留在 [verification.json](verification.json)。
