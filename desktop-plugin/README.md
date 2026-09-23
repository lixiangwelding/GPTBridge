# Desktop 插件

默认显示名 `codex无限`，映射已授权的 `asdk_app_6a848a4ede608191af51370794d3091d`，不复制凭据，也不新建公网服务。它补齐个人 marketplace 的插件身份与精简技能，不篡改客户端缓存、账号权限或闭源代码。

`/opt/homebrew/bin/python3 scripts/desktop_personal.py install --profile PROFILE_ID` 通过本机 Codex CLI 添加个人 marketplace 并安装，不重启 Desktop 或关闭其他插件。安装前备份客户端配置。`--transport stdio` 显式改用本机原生 MCP；默认不同时加载两套相同工具。

安装登记和当前聊天输入框已经刷新不是同一验收；旧目录仍在缓存时，在后续新聊天中检查。不为刷新目录强制重启正在使用的 Desktop。
