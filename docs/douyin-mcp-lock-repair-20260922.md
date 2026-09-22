# MEDIA-DOUYIN 发起的 MCP 锁与回执修复

任务：2ecfafb2-7cfe-4e09-ae4a-eeec6a0a975e。用户本轮明确授权个人 MCP 修复、精准提交推送与保留配置安装；不扩大为业务平台投稿或生产部署。

本会话写入范围仅为 personal-runtime/src/store.rs 的只读 peek_receipt、src-tauri/src/tools/personal_patch.rs 的预检/回执重读与回归、src-tauri/src/tools/policy.rs 的 shasum/sha256sum 诊断白名单以及本文件。其他 dirty 由原写者维护，不整仓 add、不回退。

已核：补丁 dry_run 原来也拿独占 source 锁；已完成请求回读先等同一写锁。修改为只读预检共享锁、完整且同参数的历史回执无源码锁回读。实际写入继续独占锁并校验 SHA；未知回执不重放；锁超时明确 patch_applied=false、receipt_persisted=false 与可重试提示。没有强制清锁或取消其他任务。

shasum 白名单遗漏通过增加两项只读哈希程序修复；没有使 privileged_executable 通用绕过白名单。外部路径、shell 与权限校验仍保留。

GUI 缺少 npx 已用现有 scripts/run_checked.py 的本机工具链环境复现并恢复 probe。其他会话正在维护 startup_path.rs，本会话不重复写。

GitNexus impact 已尝试，因本机无可用 gitnexus 包退出 1；未宣称图谱通过。人工核对仅影响个人 MCP patch 入口、Store 回执只读方法和诊断程序白名单，不改业务代码。测试、提交、安装和运行身份以本轮后继回执为准，本文初始落盘不代表完成。
