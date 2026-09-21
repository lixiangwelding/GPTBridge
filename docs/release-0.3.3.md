# Coding Tools MCP Personal 0.3.3

## 问题与修复

0.3.2 的 `/mcp` GET 对所有请求都返回 `200 application/json`。项目声明的 MCP `2025-06-18` Streamable HTTP 协议要求：客户端携带 `Accept: text/event-stream` 发起 GET 时，服务端必须提供事件流，或以 `405 Method Not Allowed` 拒绝该事件流。JSON `200` 会让按协议等待事件流的客户端收到不支持的响应类型。

现在事件流 GET 返回空体 `405`；不请求事件流的健康探测仍取得原有版本 JSON。POST 工具调用及认证逻辑没有改动。

## 取证与验证

- 2026-09-21 安装中的 0.3.2 在本地及公网收到 `Accept: text/event-stream` 后均返回 `200 application/json`，复现协议错误。
- ChatGPT 对 `@codex无限` 的实际请求到达个人版服务；审计中 `history_session_bootstrap` 和 `history_session_checkpoint` 都成功结束。此证据不能单独证明聊天界面卡住完全由 GET 响应引起。
- 回归测试先在 0.3.2 上失败（实际 `200`，期望 `405`），修复后通过，并核对普通健康 GET 仍为 `200`。
- 对照 [MCP 2025-06-18 Streamable HTTP 传输规范](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)。

真实 ChatGPT 消息完成及桌面界面状态需要升级后由用户端重新验证；本轮按用户要求不使用 Computer Use。
