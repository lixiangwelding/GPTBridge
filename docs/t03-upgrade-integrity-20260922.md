# Personal 安装器：无须替换分支的并发完整性

任务 `eb71849d-7a61-479c-bcd8-6f883c4dd4b9`；2026-09-22。用户明确授权个人 MCP 修复、限定 commit/push、保留配置安装，之后回到 V4；不扩大游戏部署、重启、SQL 或发奖权限。

本写者只修改 `scripts/upgrade_personal.py`，新增 `tests/personal/test_upgrade_noop_integrity.py` 和本文。session87、启动 PATH、权限预检及 patch 锁/回执修复仍由其原写者维护，不混提他人 dirty。

## 根因和最小修复

`_upgrade_locked` 在源应用和目标应用内容相等时，执行原生 validator 后直接返回 `already_installed`，没有重新核验三个输入。真实隔离测试复现：验证期间配置、构建源或已安装应用改变时均错误报告成功。此分支还不返回可核对的 executable_sha256。

现在原生检查后重新比较源应用、目标应用、配置树的完整 manifest；漂移拒绝签收，不恢复配置，不移动他人新应用。输入稳定时返回二进制 SHA 和 exact_configuration_preserved。安装文件与运行实例仍分开，running_process_restarted/live_service_version_verified 不因此变为 true。

## 回归与审查

- 新增5项隔离测试：首次真实红测为3失败、1错误、1通过；未删除失败记录。
- 修复后安装器全部16项通过（原11项加新增5项），覆盖重复安装、精确备份、回滚、锁竞争、软链拒绝和三类漂移。不是新运行服务或业务验收。
- 红测回执：`.artifacts/checks/t03-upgrade-noop-red-1790075896677349000/result.json`。
- 绿测回执：`.artifacts/checks/t03-upgrade-integrity-green-1790075920430225000/result.json`。
- 改前 GitNexus context/impact 2/2 成功，_upgrade_locked 上游3符号、直接调用者 upgrade，风险 LOW；影响范围限安装脚本。回执 `.artifacts/checks/t03-upgrade-impact-1790075735050046000/result.json`。
- 修复源码 SHA：`f2dac26fb21e11bcde857e2683088cc68b383c54059240bb65014010e1cf9b87`；新增测试 SHA：`db95b2c660d2653ccd31db57b5f5e3bb28525b64a1b15b75add99282c8f2e1f5`。

原 V4 遇到的 RESOURCE_BUSY 是共享源码锁占用，不能强制解锁。MEDIA-DOUYIN 的后继 patch 预检/回执回读改进另见其文档。平台级安全拦截不是本地 MCP 源码已确诊缺陷，不放宽权限、不重新包装执行被拦截操作。

提交、全量构建、安装和实例切换以本任务后继实际回执为准；本文落盘不代表这些步骤已完成。私密配置、日志和构建物不进入 Git。
