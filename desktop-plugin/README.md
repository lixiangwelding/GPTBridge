# GPTBridgePlugin

默认显示名 `GPTBridgePlugin`，内部插件标识为 `gptbridge-plugin`，映射已授权的 `asdk_app_6a848a4ede608191af51370794d3091d`，不复制凭据，也不新建公网服务。它补齐个人 marketplace 的插件身份与精简技能，不篡改客户端缓存、账号权限或闭源代码。

`/opt/homebrew/bin/python3 scripts/desktop_personal.py install --profile PROFILE_ID` 通过本机 Codex CLI 添加个人 marketplace 并安装，不重启 Desktop 或关闭其他插件。安装前备份客户端配置。`--transport stdio` 显式改用本机原生 MCP；默认不同时加载两套相同工具。

安装登记和当前聊天输入框已经刷新不是同一验收；旧目录仍在缓存时，在后续新聊天中检查。不为刷新目录强制重启正在使用的 Desktop。

## dot 与原有工作区工具

dot 在对应客户端允许使用该连接器时，沿用同一 MCP、认证和原生工具。文件读取、哈希保护补丁和持久命令由现有工具执行；`task_open` 与 `task_checkpoint` 用于任务记账与恢复，不表示必须委托外部 Agent。此说明不改变插件的显式选择要求、安装映射或权限。

单工作区先核 `server_info` 与目录；共享入口先 `workspace_list`，每次调用继续带登记的 `workspace_id`。命令或请求结果不明时查询返回的原 task/job/session/output 标识，不重新发起相同副作用。真实参数示例与失败处理见 [dot 使用说明](../docs/dot-compatibility.md)。

当前服务 initialize 保持 `tools.listChanged=false`，SSE GET 返回 `405`，没有工具目录推送通知。服务更新后，应分别核对服务端 `tools/list` 和客户端支持的刷新流程；登记成功或 `server_info` 更新不能代替输入框可见和真实调用验收。dot、原有 `@GPTBridge` 与 Codex Desktop 的缓存行为分别记录，未实测保留“未验证”。

命令执行为 `policy_only`、`sandbox_enforced=false`；单工作区旧读取允许显式外部路径，共享网关限制读取在选定真实根目录内。指南不会扩大授权、自动刷新客户端缓存或承诺特定 Codex/平台额度结果。
