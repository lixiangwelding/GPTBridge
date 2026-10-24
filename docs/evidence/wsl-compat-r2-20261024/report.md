# WSL后继修复：Git专用工具与批量规则读取恢复

本次用户提供“优化验证受阻”聊天与BLOCKED报告。已读取原生聊天记录及Windows本地审计SQLite（只读），确认shared规则走的是read_files，错误为ABSOLUTE_PATH_DENIED；git_status通过Windows Git访问UNC，错误为dubious ownership，同时误报is_repo=false、clean=true。

修复：Git专用入口统一按WSL发行版执行Linux Git，复用既有POSIX argv/身份/截止时间。Linux绝对仓内git_status路径按同版UNC映射。仓库访问失败抛Git错误，只有明确not-a-git-repository才报告非仓库，clean为null。Git diff/log/show/blame也共用该入口；不修改safe.directory。

批量读取继续限制仓库范围，新增支持仓内绝对路径。显式外部共享规则仍不由batch读取；legacy单仓只读策略允许时，错误返回精确read_file恢复指引，且permission_granted=false、automatic_retry=false；strict gateway不提示跨根恢复。保留原symlink和外部文件拒绝回归，不把权限拒绝当文件不存在。

本地8项回归通过，包含7项真实Rust共享模块单测及Windows→WSL专用Git参数计划探针；当前仓库根成功回读，无dubious ownership。Windows编译、对应fixture回归和本机安装回读待后续本批候选完成。

当前Codex仍禁止调用GPTBridge API/HTTP/CLI/探测；只读本地审计和使用独立fixtures验证，不给另一个ChatGPT聊天发消息、不重放其LOCAL-OPT-26。此轮仅修工具，不实施JAR差量优化。既有提交/推送/GitHub构建/本机更新授权沿用，安装前重新核活动作业与进程。
