# T08 个人 MCP 修复、安装与恢复验收

任务 `95b3e0cd-9fbe-40c6-ad89-54c61979c0cb`；原前端任务 `44d0030f-723d-4a94-bc7b-84b15ee2a189`。用户明确授权个人MCP修复、commit/push、安装替换；不扩大业务生产权限。

## 修复和归属

沿用已提交 `71ba38e10e731a6cea528870b5d6983448be9395`：预检使用共享源码锁，已完成的同参数回执先只读查询，真实写入仍为独占锁加expected_hashes，未知结果不重放。源码最终写者为 `2ecfafb2-7cfe-4e09-ae4a-eeec6a0a975e`；本会话不重复改、不混提其余dirty。此前回执为RESOURCE_BUSY且明确patch_applied=false/receipt_persisted=false，没有伪造落盘。

## 测试及安装

`.artifacts/media-video-mcp-f422/release-r01/release.json` 的14阶段完整回归通过，源码指纹 `eec2554fb86ce57089a7d3bd3f45abfd49c9912f94afe2539ec552cceafe1c49`；早期失败保留，不重复叠跑。其commit/remote为b29df973f44ee00024fee1b339142d76f1325c08。后继build job62de6c82-f195-4d10-bcab-3a0c09a0b451确认相同指纹，产物a9fbc18。制品包含其他写者声明的dirty，不声称仅一个提交可重建全部包。

本人安装复验job `d560ec7d-0827-4e14-bcb0-ff00d53bce21` 在受管锁下核候选/安装均为 `a9fbc18cad8ad723ef9a87b1a30405904dc18a56ca6997a5908621a3f19655c3`，调用官方upgrade_personal.py --apply，返回already_installed及原生配置/33工具合同通过，未降级或修改业务。

## 正常激活与配置核对

本人job `e148f36e-76f3-4c93-8943-7490bd1886f0` 持安装互斥并核a9fbc18，对唯一个人bundle正常退出再打开。监听关闭/恢复得到确认，未强杀进程、未停止业务服务。回执 `.artifacts/t08-mcp-recovery-95b3e0cd/activation.json`。

切换前实际task_status(limit=1)仍返回jobs.limit=50；切换后当前连接返回1条、limit=1、truncated=true，旧任务可恢复。重启后配置字节SHA发生变化，但job314410f3-9f2c-4394-974a-d53c0e25e0f0与精确安装备份递归比较：全部JSON值一致、变化字段为空；没有输出密钥或恢复旧配置。不能把字节相同和配置值相同混为一谈。

## 原交付恢复与验收边界

job `00d77a52-59a1-426a-8ea7-0259ded2391b` 等待同一source独占锁，以create-only方式补存并逐字节回读原前端RUN的STATUS.md、HANDOFF.md、results.json、cross-requests.json。没有绕锁或覆盖已有结果。这是原revision2证据补存，不是新业务PASS，前端路由/权限/截图/性能未验项仍保留，不晋升public current。

真实dry_run在6ms完成并返回预条件、未写文件。新实例初次原生apply_patch仍遇RESOURCE_BUSY，失败回执保留，排队受管写成功没有被冒充为原生补丁PASS。后继请求 `t08-mcp-20260922-live-node-receipt-append` 已实际写入，change_id `7e694c70aeea494bac558dca4e0defaa`，dispatch31ms、receipt_persisted=true；完全相同请求再次调用得到deduplicated=true、相同change_id和相同after_hashes，dispatch2ms，没有重复追加。这两个独立真实验收现已通过。全过程不强清锁、不取消他人作业，不重包被安全层拒绝的进程查询。

本文件为本会话唯一个人仓提交文件；Git与远端回读保存在 `.artifacts/t08-mcp-recovery-95b3e0cd/delivery.json` 及任务checkpoint，生成文档不代表业务闭合。

后继真实工具：job `9baadded-93c5-444d-ad8c-afb8bccbed13` 的裸 `node --version` 返回 v26.3.0/exit0；read_output保留相同task_id、request_id和job_id，未借助解释器复制或权限绕过。
