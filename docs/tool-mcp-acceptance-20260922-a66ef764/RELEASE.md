# Coding Tools MCP Personal 修复与安装核验

记录时间：2026-09-22T20:49:44.924964+09:00。任务：a66ef764-4614-4053-9273-c4353fa060f8。

## 当前结论

修复代码已合入独立仓 main，已推送，新应用已安装；本任务独立核验当前连接已使用修复后的输出身份逻辑。未重复安装、未回滚其他会话源码，未修改业务项目或全局代理配置。

源码提交：`6be33f257fd64abc41a6c370a926f9781d92ac55`。并行授权集成者提交了全部19个继承/本任务修复文件；本任务核对并推送，不冒领其提交操作。源码指纹：`eec2554fb86ce57089a7d3bd3f45abfd49c9912f94afe2539ec552cceafe1c49`。

## 已修复与复核

- read_output返回持久job_id、task_id、request_id，原作业读取已不再丢失归属；不改变UTF-8分页、权限与任务选择。
- Python平台名称及macOS venv启动入口测试夹具已修正；保留路径越界、程序冒充、陈旧SHA与幂等负例。
- 继承历史会话隔离、修订顺序、检索/审计计时、紧凑Skill目录和全量测试入口修复；后续PATH、权限预检、锁/回执、Git与列表修复均保留。
- 这次Git推送失败已定位为直连路径不可用：进程与Git无代理，但系统配置127.0.0.1:7897。单次Git参数使用现有代理后ls-remote和push实际成功，不写全局/仓库代理，不修改认证，不强推。

## 本任务完整回归

14/14阶段通过。桌面Rust完整回归412项、Python43项；Svelte为0错误0警告，桌面构建通过。源指纹测试前后相同，安装后独立检查仍匹配。定向子集与完整套件不累加；既有Rust警告、此前370/2、372/1及HashSet.push编译失败记录保留。

回执：`.artifacts/selftest/1790076977831398000/result.json`；原job：`2a8006b1-786e-4185-bc03-02cfba068c09`。同窗一次Session terminated后恢复原task/job，没有重跑完整套件。

## 推送与安装

本任务push job `d3769d2b-ae75-406f-a6f5-e80cda961d88` 退出0，将origin/main由b29df973快进至6be33f2。

安装由并行授权安装者执行，本任务避免重复安装并完成独立验证：

- 应用：`/Users/didi/Applications/Coding Tools MCP Personal.app`，版本仍0.3.4。
- 安装二进制SHA256：`a9fbc18cad8ad723ef9a87b1a30405904dc18a56ca6997a5908621a3f19655c3`。
- 官方升级器只读plan确认已安装与当前候选逐文件一致，app_files_will_change=false；树摘要为`b682e48ca259a1d49afa7bb6485691c98f80e4c8fb8da7ae5f63e1e1935ae53c`。
- 已安装程序的只读原生检查：1份配置成功解析、33工具合同一致，检查期间配置字节不变；本检查没有写配置或启动服务。这不是原安装事务的逐字节保留证明。
- 当前真实连接读取旧job输出页，已返回正确job/task/request身份；原生exec直接运行node --version得到v26.3.0，无辅助包装器。

安装事务的原job状态读取被平台拦截，未走其他接口读取该回执；当前PID映射没有独立验证。以上安装与运行结论来自本任务独立的制品/原生校验/实时接口结果，不来自被拦截输出。

## 安装后的真实保护验收

新建post-install夹具最初两次遇到RESOURCE_BUSY，均明确patch_applied=false、receipt_persisted=false；拒绝记录保留，未强解锁或取消其他作业。正常写锁释放后，同一个稳定请求成功创建本目录live-fixture-r2.txt，change_id为`dce10b110e3c4168b0241a9c5b4e4451`；原样重放返回同一change_id和deduplicated=true。另一个错误SHA请求返回STALE_FILE、patch_applied=false；原生回读文件内容不变，SHA256为`4c7019a90d3d389b4cdf6fe87bfb74a49a566bf484b0217fe47e2b4878d51dce`。因此安装后保护写入、幂等重放与错误SHA拒绝已实际通过。

真实task_status(limit=1)也已只返回1条作业，jobs.limit=1、truncated=true，修复后的恢复输出边界已在当前连接验证。

## 仍保留的未验边界

受控多会话竞争、冷热长尾、完整断线演练、FRP/边缘502唯一根因、Windows/移动端/新OAuth、正式签名与真实升级回退专项未关闭。不能宣传原生$下拉，native_dollar_picker仍为false。

完整结构化结果见本目录`results.json`的latest；previous_turn保存前轮失败和状态。STATUS/HANDOFF/cross-requests中的原文字段保留为历史，不覆盖本次实际成功证据。
