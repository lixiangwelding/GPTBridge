# APP-GM 发起的个人 MCP 恢复

任务：39f40a5c-2281-48ad-8e44-8e59ef1da7bc。用户本轮明确授权个人 MCP 修复、精准 commit/push、保留配置安装替换；APP-GM 仍不新增发放、部署、重启或数据库写授权。

本会话变更只涉及 `src-tauri/tests/call_tool_security.rs` 中 traversal / absolute / protected Git 三项测试的请求参数及未写入断言，以及本交接文档。该文件已有其他会话修改，全部保留；提交仅暂存本轮独立增量，不夹带其他 dirty。运行时 jobs/dispatch、权限预检与 patch 锁修复由现有写者维护。

实际复现：运行实例 `read_output` 读取本任务作业 `171dbb35-daaf-45a7-a299-a3b80e11ed1d` 时返回 `task_scope=null`；当前未提交 jobs.rs 已有从权威行回填 job_id/task_id/request_id 的后继修复，不能再重写一套。原日志/账本诊断读取被 OpenAI 调用前安全检查拦截；这不是 MCP 的错误回包，不通过调整权限或包装器绕过。

本轮完整 Rust 回归原始失败回执：`.artifacts/checks/app-gm-mcp-rust-release-20260922-1790075781093612000/result.json`。其中 call_tool_security 25 项有 3 项失败：新补丁请求校验先返回 REQUEST_ID_REQUIRED，旧测试没有走到期望的路径拒绝。修复测试时补齐 request_id/expected_hashes，保持 security/policy/PROTECTED_PATH 断言，新增外部夹具与 Git 配置字节不变断言；不把失败断言改成接受请求。

构建和安装必须绑定当前源码与实际制品；其他会话已取得的同源回执可以按 SHA 复用，不能重复累计套件。此文初次落盘不代表已提交或安装。图谱工具/技能在本仓发现范围内未找到可调用入口，未声称完成图谱影响分析；本轮只改测试输入，生产逻辑由原写者负责。
