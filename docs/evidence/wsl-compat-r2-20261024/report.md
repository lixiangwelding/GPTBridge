# WSL后继修复：Git专用工具与批量规则读取恢复

本次用户提供“优化验证受阻”聊天与BLOCKED报告。已读取原生聊天记录及Windows本地审计SQLite（只读），确认shared规则走的是read_files，错误为ABSOLUTE_PATH_DENIED；git_status通过Windows Git访问UNC，错误为dubious ownership，同时误报is_repo=false、clean=true。

修复：Git专用入口统一按WSL发行版执行Linux Git，复用既有POSIX argv/身份/截止时间。Linux绝对仓内git_status路径按同版UNC映射。仓库访问失败抛Git错误，只有明确not-a-git-repository才报告非仓库，clean为null。Git diff/log/show/blame也共用该入口；不修改safe.directory。

批量读取继续限制仓库范围，新增支持仓内绝对路径。显式外部共享规则仍不由batch读取；legacy单仓只读策略允许时，错误返回精确read_file恢复指引，且permission_granted=false、automatic_retry=false；strict gateway不提示跨根恢复。保留原symlink和外部文件拒绝回归，不把权限拒绝当文件不存在。

本地8项回归通过，包含7项真实Rust共享模块单测及Windows→WSL专用Git参数计划探针；当前仓库根成功回读，无dubious ownership。Windows全部Rust测试目标编译通过，WSL定向14项回归通过，25项调度器Python测试通过；前端检查、Node测试和NSIS打包通过。完整Rust测试suite没有运行。

当前Codex仍禁止调用GPTBridge API/HTTP/CLI/探测；只读本地审计和使用独立fixtures验证，不给另一个ChatGPT聊天发消息、不重放其LOCAL-OPT-26。此轮仅修工具，不实施JAR差量优化。既有提交/推送/GitHub构建/本机更新授权沿用，安装前重新核活动作业与进程。


交付结果：源码提交 `e70a34d9152563fdacb5dc84472a51de2463d073` 已推送并核对远端SHA；[Windows构建40082641991](https://github.com/lixiangwelding/GPTBridge/actions/runs/40082641991) 成功。本机安装程序退出0，安装版SHA256为 `3080251015affb1f05fb51e08da79b57952fc7abeadbfb4eaa3d5dd212808503`，新进程PID4304；原配置MCP经原生配置界面启动，UI显示running，OS回读127.0.0.1:38766归属4304，三份Node上游及FRP子进程归属新程序。

替换前重新核对活动作业0、调度器进程无；备份目录 `C:\Users\99321\AppData\Local\GPTBridge-backups\wsl-compat-r2-20261024-e70a34d`。更新后14份队列文件字节一致，profiles.json由原生服务格式化，JSON语义一致。没有启动Windows调度器，没有执行业务任务或重放未知写入。

30份提示词的Markdown及HTML已同步补充read_file恢复方式和Git宿主核验，30份正文逐字一致、插件标签完整。该材料留在外层codeVerify仓库，不混入GPTBridge源码提交。GitNexus无注册索引，impact/detect_changes无法提供图分析，已使用实际调用入口、精确diff和定向测试复核，未重建索引。

原ChatGPT聊天的插件端到端复验仍为 `NOT_RUN_CODEX_BOUNDARY`。本轮证据覆盖源码、Windows构建、独立Windows→WSL探针、安装身份及服务进程/监听；不把这些证据写成LOCAL-OPT-26源码优化已完成。
