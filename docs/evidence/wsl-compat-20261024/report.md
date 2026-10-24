# GPTBridge Windows WSL兼容修复交付

2026-10-24，源码修复、提交推送、Windows构建、本机安装和MCP服务恢复已完成。ChatGPT经插件实际执行业务没有由Codex调用验证。

## 已交付

- 源码提交：`cfbd0150ac58604df31fda2d3f0fa909255b1c22`，独立GPTBridge仓库main，远端SHA已回读。外层codeVerify仍为master。
- [Windows构建40074423075](https://github.com/lixiangwelding/GPTBridge/actions/runs/40074423075)成功，8分33秒；完整Rust测试目标编译，10项筛选回归通过，25项Windows调度器测试通过。并未执行整个Rust测试套件。
- 本地7项Python回归通过，含真实共享Rust模块6项单测、Windows→Ubuntu-24.04只读身份/Node探针和Windows relay结束后的自建Linux子进程回收；另7项已有Svelte表单契约通过。
- 已安装新程序：`C:\Users\99321\AppData\Local\Programs\GPTBridge\coding-tools-mcp-personal.exe`；安装器exit=0，程序SHA256=`f0bc9b3abfab9b6baada0f1d9756bca2a89c25f6127ecd32ece20e9f0ec08b1d`。
- 本机MCP界面显示运行中；127.0.0.1:38766由新程序PID58732监听；3个既有Windows Node MCP与FRP子进程已启动。进程/端口不冒充公网、插件鉴权或业务调用验收。
- 原profiles.json字段完全一致，启动服务后仅重新格式化；8份队列/原请求文件字节一致。未启动独立任务调度器、未执行其他队列任务。
- 旧程序和私密配置回滚副本保存在本机用户AppData下 `GPTBridge-backups/wsl-compat-20261024-cfbd015`；私密配置没有提交Git。
- 原30份提示词本地md/html已增加WSL宿主前置，仍保留两个应用标签；这些属于外层codeVerify文档，未夹带入GPTBridge提交。

## 行为

WSL UNC仓库自动按对应发行版执行Linux命令，共用POSIX参数解析、同版cwd、绝对读取路径转换及Linux用户技能根。可用现有NVM Node/npm；正常Windows工作区保留原行为。两种执行模式共用随机身份清理与Linux截止时间，不关闭WSL或无关进程。

## 未验证边界

Codex规则禁止调用GPTBridge及别名、探测或代理转调，因此未发起server_info/exec_command等插件协议调用，未操控ChatGPT，也未进行公网HTTP探测。ChatGPT侧真实插件请求、鉴权与业务输出仍待其实际使用回读，不能写为PASS。没有部署codeVerify或游戏服务；本机安装更新1次。

## 回滚

先核对新进程的活动作业和连接归属，再关闭该Windows应用，以保留的旧exe恢复精确目标文件并核SHA。配置已语义一致，不自动用备份覆盖用户后续修改；队列和原请求不删除、不重提。禁止wsl --shutdown、批量终止进程或重放UNKNOWN请求。

## 证据

同目录 candidate.json、installation.json、install-preflight.json、runtime-readback.json、windows-summary.json、local-tests.txt、frontend-contracts.txt。GitHub原始日志仅作为本机取证文件保留，不将其旧环境变量或整段输出当必要提交内容。
