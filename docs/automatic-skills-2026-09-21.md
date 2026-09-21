# 自动 Skill 发现与模型选择（2026-09-21 UTC）

## 交付范围

承接个人仓库 main 的 `628debf34de6d5ae5ddc2cdd97ff93b2fb0897e8`，只增强 Skill 发现、模型提示和兼容入口，不重做运行器、不修改 FRP、认证或用户配置。用户正常连接插件并描述工作目标，不必记住 Skill 名。

这次完成的是服务器提供目录与引导模型选择，不是服务端运行第二个模型，不是原生输入框菜单，也不保证任意客户端一定遵循提示。未执行当前用户 ChatGPT 新服务连接的端到端模型选择评测，未安装或重启在线程序。

## 入口与数据流

单仓库 `initialize.instructions` 自动附加轻量目录；`tools/list` 仅给现有 `invoke_skill` 的描述附加同样目录，工具数量不增长。缓存命中计数不进入描述，避免相同目录每次改变 schema 文本。

`task_open`、旧 `history_session_bootstrap`、`server_info` 成功结果附加 `skill_discovery`。失败原样保留；普通读文件、任务轮询、执行和 checkpoint 不附加目录。扫描问题不会把已经创建成功的任务伪装成创建失败。

多仓库网关初始化只说明选库流程，不合并或扫描全部成员目录；明确 `workspace_id` 后由该成员的任务启动/信息查询返回目录。手动查找和读取依然经过原鉴权、路由、范围与权限校验。

模型先依任务选择最小相关集合（通常初始 1–3 项，强制项目流程优先），再读取正文及需要的参考。没有匹配时正常完成任务，不要求用户提供技能名。清单缺失/不完整/不相关时，模型使用 `automatic_only=true` 自行查找、按返回游标分页。

## 性能与兼容性

目录最大 32 项，条目 JSON 合计上限 8KiB，每条描述最多 384 UTF-8 字节。测试验证包含长中文和 emoji 描述的目录快照小于 12KiB；这不是全包或 token 上限。超限明确 `partial`，保留完整元数据分页入口。

复用已有每仓库缓存、文件身份/mtime/ctime 失效和逐次路径检查；没有新增全局会话状态或定时扫描器。缓存仍不保存正文。技能增删、元数据修改、仅手动标志改变均热更新。动态目录在初始化/显式目录请求/任务启动时刷新，不承诺绕过客户端工具清单缓存主动推送。

`disable-model-invocation: true` 从自动目录与 `automatic_only=true` 列表/搜索剔除，手动调用仍可读取。旧调用不传新参数时语义保留；布尔类型严格验证，游标绑定自动/手动模式。旧游标在服务升级后失效时应从新搜索开始。

## 审查结论

本轮依据实际 Git diff、新增模块全文及隔离回归人工审查，未发现剩余阻断项。未声称获得完整 GitNexus 调用图或独立模型审查。辅助规划 CLI 的组合调用被平台拦截后没有重试或绕过；原生 apply_patch 因当前连接器 schema 缺少服务端必需 request_id 被拒绝、没有落盘。本轮通过已授权本地源码操作核对 Git 基线和文件 SHA 后精确修改；没有改变工具安全策略或连接器配置。

检查重点：目录不可被当成已加载技能，描述与正文是不可信任务资料；自动发现不授予执行权限；未认证 HTTP 不泄露目录；多仓库不混合 ID；无静默全库截断；冷/热缓存不让工具说明抖动；失败任务不追加目录；大目录与8并发无全局工作区切换。

## 实际验证

最终命令：`python3 scripts/selftest_personal.py --suite all`

- 14/14 阶段 passed，全部 exit 0。
- 完整 Rust 库：test result: ok. 260 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.17s
- 新增 14 个自动发现回归 + 1 个真实 loopback 鉴权 HTTP 多仓库回归，均属于完整 Rust 库子集，不重复累计。
- 旧运行时任务恢复、并发、幂等、取消/超时、源锁与输出测试及前端检查、桌面构建仍通过。
- 首轮夹具缺少 task_open request_id，修正夹具后基线 7/7 真实缺失行为失败；实现后 7/7 通过，再加入8项边界/HTTP检查进入完整回归。保留所有红绿回执，不弱化生产幂等要求。

最终回执：`.artifacts/selftest/1790005678881920000/result.json`

最终源码前后指纹：`5d37e7b827a7662bae62e8e9c3884ca51919a473076d1de8e95a85166b9e583a`。文档更新不影响此指纹；提交前再次验证。

## 启用与未验证范围

源码构建通过不等于在线替换。需连接到包含本改造的 MCP 二进制，并让客户端刷新工具定义/重新建立会话；当前长任务不强制重启。本轮没有 push、GitHub Release、安装替换、公网 OAuth 实连、Windows 实机或真实大模型选技命中率验收。

## 设计依据（主源，核对日期 2026-09-21）

- Agent Skills：先披露名称和描述，再由模型按任务激活，支持把目录放在专用工具描述中。https://agentskills.io/client-implementation/adding-skills-support
- MCP 2025-06-18：initialize 的 instructions 是客户端可使用的提示，不是服务端强制行为。https://modelcontextprotocol.io/specification/2025-06-18/schema
- OpenAI：工具描述需说明适用情形，真实模型选择仍需精确率/召回率评测。https://developers.openai.com/plugins/guides/optimize-metadata
