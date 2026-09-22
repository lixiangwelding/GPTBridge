# MEDIA-IMAGE 发起的工具链恢复验收

task_id: `fffdb2cf-0cfb-401e-acf7-9d20d405751f`

本轮用户明确授权个人 MCP 修复、精准 commit/push 和安装替换；不新增生图付费请求、业务生产发布或 RDS 修改授权。

## 当前事实与协作边界

- 起始独立 main 为 `a76ca20f8d963289935f9e6b5fb5817888f2029e`；存在其他会话未提交改动，不能全仓 add 或覆盖。
- `startup_path.rs` 的 GUI 工具目录补全已存在；`dispatch.rs` / `exec.rs` 的权限预检由已有写者维护。本会话不重复修改这些文件。
- `personal_patch.rs` / `store.rs` 的共享预检锁及已完成回执回读由 `2ecfafb2-7cfe-4e09-ae4a-eeec6a0a975e` 维护；本会话不抢写。
- 上轮 npm 缺失及绝对 Node 路径拒绝为真实失败；权限 granted 不是可执行路径验证通过。绝不以包装器、复制解释器或取消工作区边界绕过拒绝。
- 本轮通过已有 `scripts/run_checked.py` 项目验证入口执行了 mcp-probe-kit 4.0.1 的 Bug 工作流。另一次进程只读查询被 OpenAI 安全层拒绝，未更换通道重放。
- 已读其他写者全套回执 `job:ad162a79-5765-4e7a-85ba-ad4614befe8e:stdout`：full-rust-regression 因 `tests/exec_path_regression.rs:161` 对 HashSet 使用 push 而编译失败；须核当前文件是否已修，再由该写者修正，不重复叠跑全套。

## 本会话范围

本文件、本会话独立工具回归与验收产物，以及原 MEDIA-IMAGE 域修复。共同运行时代码按现有写者归属，安装必须核唯一构建者、冻结输入和实际安装/运行 SHA；不拿磁盘替换冒充活动进程已更新。下方进度以追加回执为准。

状态：工具集成、提交、安装与生图全套尚未闭合。
