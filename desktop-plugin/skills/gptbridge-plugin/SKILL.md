---
name: gptbridge-plugin
description: 使用 GPTBridgePlugin 处理本地项目、日志、持久任务与文件交付；用户提到 GPTBridge、旧 codingtoolsmcp 或本地任务续接时使用。
---

# GPTBridgePlugin

先调用 server_info 确认实际工作区、运行版本和可用工具，再用 task_open 创建或恢复明确的 task_id。命令与写入带 task_id 和稳定 request_id；补丁先读文件并带 expected_hashes。同文件冲突重读合并，保留其他会话改动。

文档和生成文件写入已确认的本地工作区，写后核验存在性与内容，最终返回绝对路径。长命令查询原 job，不重复提交未知结果；结束前 task_checkpoint 保存实际进度与未验证项。

安装版本、正在监听的版本、服务端 tools/list、插件目录和当前聊天能否调用分别核验；任何一层成功都不自动证明下一层成功。
