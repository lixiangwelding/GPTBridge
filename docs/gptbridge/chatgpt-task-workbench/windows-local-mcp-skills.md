# Windows 本地 MCP 与技能修复

目标是 Windows GPTBridge 的本地 MCP 与工具/技能页面。Mac 不访问、不修改。

## 本地 MCP

现场配置原先没有 upstream_mcps，界面显示“还没有本地 MCP”。已接入并运行当前 Windows 机器现有的 Playwright 0.0.83、jshook 0.4.2、drawio 1.7.0；直接使用已安装的 Windows Node 与对应入口，不通过 WSL 运行这些服务器，不下载另一套依赖。

三项独立 Windows stdio 握手与 tools/list 分别返回 25、8、8 个工具。测试只读取工具目录，没有调用业务工具、打开浏览器或联系 GPTBridge。三个测试进程在 EOF 后均已退出，进程回读均为 0。最终配置应用后，后台回读三个 Windows Node 服务各运行一份。目录握手与进程证据不能替代每项工具经 ChatGPT 转发的业务验收。

接入配置仅修改 Windows 项目 runtime.upstream_mcps，保留原 trusted、advanced、端口、凭据、FRP 及其他字段。前缀为 playwright、jshook、drawio，公开名为 前缀__原工具名。不启用原本禁用的公司或数据库连接，不将交互式 Computer Use 会话再套入上游代理。

## Windows 技能

修复前实机页面显示 474 项技能并报扫描边界。正式技能目录测得约 4,144 个目录项，原上限为 4,000；现改为 8,192，保持深度 5、最多 1,000 项技能、读取 32 MiB 等原限制。Windows 测试覆盖 8,192 项及第 8,193 项，不以无界扫描解决截断。最终安装版原生目录显示 **477 项技能、52 个内置工具**，扫描截断提示消失。

原实现把 Windows 原生相对路径中的反斜线交给仅接受 POSIX 引用的检查器，导致嵌套 canonical 与共享参考文件被拒绝。修复在原始输入和 canonical 根检查后，将原生路径组件序列化为正斜线引用；原始反斜线、越界和敏感文件仍拒绝。来源 URI 同时采用正斜线，避免错误的 %5C 引用。

Windows 追加回归包括嵌套与共享正文读取、来源 URI、越界/敏感路径拒绝及扫描边界。独立案例在临时目录中运行，不调用生产 Bridge 工具。

## 验收层级

最终安装源码 `955a22306bbfd3cfe52ecc170c3c710e20c76c85`，[Windows 构建 39783260329](https://github.com/lixiangwelding/GPTBridge/actions/runs/39783260329) 成功。21 Node、19 Python、6 Rust 定向测试实际通过；全量 Rust 未执行。详情预览最后一次 Computer Use 动作报错，用户随后要求停止界面自动化；没有取得最终原生详情通过证据。安装、配置与原生目录记录见 [delivery.json](delivery.json)。Skill 目录/原文读取不等于逐项 Skill 业务执行；本地 MCP 握手不等于全部工具已执行业务。

原 20 任务批次与 requestId 保留，不重新提交。日志版独立调度器曾在唯一 ChatGPT 模式窗口发现一个可见“停止”按钮而等待；升级前已停止，当前仍 stopped，消息发送标记和回执均为 0。Codex 直接 GPTBridge MCP/HTTP/发送脚本硬拦截保留，只有独立调度器控制 ChatGPT。
