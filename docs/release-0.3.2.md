# Coding Tools MCP Personal 0.3.2

## 功能

新增原生只读 Skill 桥接：`list_skills`、`search_skills`、`read_skill`、`invoke_skill`。复用现有本地全局/项目技能目录，支持中文名、目录别名和消息发送后的 `$技能名` 查找；元数据按需加载，正文与参考资料提供路径、行号和 SHA。多仓库入口继续按 `workspace_id` 路由，无需第二条 FRP 或额外常驻进程。

`$` 不是 ChatGPT 输入框原生下拉扩展；引用为可核对的普通来源而非伪造平台 filecite。Skill 读取不执行脚本，不自动连接其声明的外部 MCP，不扩大文件写入或网络权限。

## 验证

命令：`python3 scripts/selftest_personal.py --suite all`。

- 14 阶段全部通过，整体退出 0。
- 完整 Rust 库：235 通过，0 失败，0 忽略。
- Skill 专项：28 通过，属于上述完整库子集。
- 前端测试、Svelte 检查、前端构建及 macOS 桌面开发二进制构建通过。
- 前后源码指纹：`52d9c494251ded21bc962bd4507cf6edcf8a9b8d19aca25d4dde1cd89873bae3`。
- 私有原始回执：`.artifacts/selftest/1789994741430143000/result.json`。

## 升级与配置说明

本次提交不修改用户正在运行的旧应用、认证、FRP 或配置文件。旧服务替换与完整配置备份因平台安全检查未执行。请勿把 `.personal-home` 中早期隔离导入的端口/禁用隧道设置直接覆盖到正式配置；该副本不是对原配置的无损升级结果。

本版本包含源码与自测结果，不宣称完成原生 GUI 人工操作、ChatGPT 用户端实际连接、Windows 实机或签名安装包验收。

使用说明见 `docs/local-skills.md`，逐项审查与既往失败记录见 `docs/local-skills-review-20260921.md`。
