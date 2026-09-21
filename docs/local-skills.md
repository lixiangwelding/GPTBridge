# 本地技能桥接：聊天只连接一个插件

## 支持到哪一层

个人版提供原生 Rust 技能读取器，不需要另外运行 FastMCP/Python 服务，不复制技能原件，不修改 Codex 配置或当前 FRP。

在连接到新版个人 MCP 的 ChatGPT 对话中，发送 `$技能名` 后，模型可以通过 `search_skills` 检索并用 `invoke_skill` 加载正文。返回结果包含来源路径、行号、SHA256 和分页游标。这里的“调用技能”是读取说明供模型按当前任务使用，**不是自动执行技能目录里的脚本**。

**输入框还未发送消息时的原生选择器不是本功能。** 官方当前文档区分 ChatGPT 的 `@` 技能选择与 Codex CLI/IDE 的 `$` 选择。MCP 工具协议不规定输入框菜单；仅改本地服务无法保证把所有本地技能注册进 ChatGPT 的原生 `$` 下拉。此版 `native_dollar_picker=false`，不能把工具返回的来源位置冒充 ChatGPT 原生文件引用。

官方参考（核对日期 2026-09-21）：
- https://developers.openai.com/codex/skills/
- https://modelcontextprotocol.io/specification/2025-06-18/server/tools
- https://agentskills.io/specification

## 自动发现的目录

- 当前已注册仓库的 `.agents/skills`、`.codex/skills`。
- 本机用户目录的 `~/.agents/skills`、`~/.codex/skills`。
- 额外全局库可在**本地服务启动环境**的 `CODING_TOOLS_SKILL_ROOTS` 中配置，使用系统路径分隔符。不接受远程工具参数新增目录。

`CODING_TOOLS_PERSONAL_SKILLS=off` 可关闭桥接。所有连接到该入口且被授权的调用方都可能读取已开放的全局技能，开放前应检查是否包含不应共享的信息。本版不读取 Codex 登录令牌，不自动复用外部 MCP OAuth 会话，也不把 Codex 的其他配置策略声明为已继承。

扫描有目录数量、深度、单文件和总读取量限制。截断结果显式返回 `scan_truncated`；损坏或不支持的 Skill 计入 `skipped_entries`，不以错误堆栈回显其正文。每次调用重新检查目录，增删改技能无需重新构建或重启程序。

元数据使用固定版本的 `serde_yaml_ng` 解析 `name`、`description`，支持中文名称、引号、UTF-8 BOM 和折叠描述。此兼容实现不强制技能目录名等于 YAML name；可按两者搜索。不是对 Agent Skills 标准所有可选扩展的完整运行时实现。

## 对话怎么写

```text
@个人版插件 $前端开发skill 检查这个页面
@个人版插件 搜索和代码审查相关的本地技能
@个人版插件 列出这个仓库可用的 Skill
```

多仓库入口先 `workspace_list`，再为每次技能工具调用传 `workspace_id`。一个仓库的 skill_id 不能拿到另一个仓库使用；全局技能也以当前仓库作用域产生 ID，不用切全局 cwd。

模型调用顺序：

1. `search_skills(query="$技能名")`，先拿摘要。
2. `invoke_skill(skill_id="返回的ID")`，读取正文。
3. 遇到参考文件，`read_skill(skill_id="...", file="references/guide.md")`。
4. 遇到分页，保留 `sha256` 并以 `offset=next_offset`、`expected_sha256=sha256` 继续读取。文件变动会拒绝旧分页，不能拼接不同版本。

重名技能不静默合并或选第一个；先给出候选，再用准确 ID。不完整名称用于搜索，不直接执行模糊命中的技能。

## 引用与读写边界

Skill 和参考文件只读。允许指向同一已授权技能库内的 `../shared/...` 公共资料；拒绝逃离库的父目录路径、未授权软链、凭据类文件名、二进制和超大文件。项目技能根不能借软链偷偷变成另一个仓库的目录；明确授权的全局库间软链可以复用。

正文返回的是普通来源引用：路径/行号/SHA，不能伪造平台 `filecite`。引用内容不授予额外权限；原有执行器、审批、文件修改和任务恢复规则继续有效。技能声明了某个 MCP 名称，不等于服务已经连接或认证。

macOS/Unix 使用逐级 no-follow 打开普通文件，拒绝 FIFO 等特殊文件。Windows 使用较弱的路径/文件检查，本轮没有 Windows 实机竞态验收；这不是操作系统级安全沙箱。

## 本地检查，不启动服务

```bash
python3 scripts/personal.py check-skills --workspace /absolute/repository
python3 scripts/personal.py check-skills --workspace /absolute/repository --query '$前端开发skill'
python3 scripts/selftest_personal.py --suite all
```

命令输出技能元数据，不执行技能脚本，不开启监听器或隧道。当前已运行的旧 MCP 不会热替换；新版能力需要客户端连接到新版服务并刷新工具清单后才可使用。保持正在执行的旧任务不受影响，不要为此覆盖旧进程或旧 FRP。
