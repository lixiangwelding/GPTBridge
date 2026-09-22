> 后继更新：用户已授权修复/提交/推送/安装；代码6be33f2已推送，14/14回归通过，安装及实时接口已独立核验。最新事实见 [RELEASE.md](RELEASE.md) 与 results.json.latest。下方为前轮历史快照，不再代表当前未完成状态。

# TOOL-MCP 可执行交接

任务 `a66ef764-4614-4053-9273-c4353fa060f8`；工作区 `/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal`。恢复使用本任务 task_id，不复制其他 session_key，不自动 bootstrap 历史。

## 已完成的实际代码工作

`personal-runtime/src/jobs.rs` 的 Store::job_output 已从本次调用已经读出的权威作业行返回 job_id、task_id、request_id。未新增数据库读取；未改分页算法、任务选择、状态机或权限。原已安装实例的 read_output 曾返回 task_scope=null，而 task_status 对同一 job 能返回正确 task_id，这是本轮真实发现，不是推测。

`src-tauri/src/tools/session87_tests.rs` 新增 `session_handoff_output_pages_preserve_persisted_identity`：验证绑定与未绑定作业、调用方不能重标记 owner、5字节中文 emoji 输出重组。最新定向回归实际执行 1 项，1通过，300项过滤；不是全量301项通过。

`src-tauri/tests/call_tool_security.rs` 修正平台解释器名称并增加真实 command_ok 断言。`src-tauri/tests/exec_path_regression.rs` 的 venv 测试先解析真实 Python，而非把 macOS 启动 shim 改名链接；仅在临时测试 ToolContext 中明确允许该唯一解释器，保留 sys.prefix 与外部程序冒充/越界负例。生产白名单、活配置、全局 PATH 未由本任务修改。

本任务曾错误使用 HashSet.push，已诚实保留 E0599 编译失败。自己的 push→insert 补丁仅收到 RESOURCE_BUSY；稍后回读发现另一写者已改为 insert。未覆盖该修改，未冒领其成功回执。当前7项路径回归全部通过。

## 最近成功观测的四文件 SHA（不是最终原子快照）

| 路径 | SHA256 | 来源 |
|---|---|---|
| personal-runtime/src/jobs.rs | edde94910aed139099f19c2d437de19148f91edecad42eb84268204b0c160310 | 本任务保护补丁 f6beb32b118045da9733b4786ef76d42 |
| src-tauri/src/tools/session87_tests.rs | 78707580b170d5bef357afba367a3af09ca7c90ea63b3550d069c09bd1f66135 | 同上 |
| src-tauri/tests/call_tool_security.rs | 9aa51b1829dd0050b0daa342348538bc89d483e89ecfce9e1714c117f4b5f11b | 同上 |
| src-tauri/tests/exec_path_regression.rs | c48a0708db51c67a6cc920f4f4075cb6c285adeb07fc40b7bac1663f63876504 | 本轮后续原生 read_file；最终 insert 写者尚未绑定 |

## 真实客户端验收证据

独占夹具 `fixture.txt` 留存。创建 change_id `ff592d5283a4423eb5f6f29a3ff7648c`；更新 change_id `a249d4f29e374b2d97bba09b4e613e94`；两个请求原样重放均返回同一 change_id 和 deduplicated=true。故意旧 SHA 的另一请求返回 STALE_FILE、patch_applied=false，回读仍为 value=after，SHA `db23370e0e8a046ef35d0d16cdd53723c9471c1060c00d6ef26c0aa6f3b3095a`。

中文输出 job `3a90173e-a37d-4487-98d1-04a3b45fc195`：15字节，以limit=5按offset 0→3→6→10→15无损取回；同request_id执行重放返回原job_id、deduplicated=true。其原运行实例输出页丢失task_scope，已修源码但未安装。超时 job `eb96c1fa-a929-4c6d-8a64-db7fc9e6dba8` 进入 timeout，终点打印未执行；取消 job `b08291b8-a208-46ad-90f1-93f2dedfc71f` 经原生kill_session进入cancelled。均仅本任务无害作业，未取消服务或别的任务。

task_open、task_status、checkpoint 已实际可用；这不等于真实断网后恢复完整PASS。受控多ChatGPT同文件竞争、冷/热缓存和并发checkpoint长尾仍未做，不把共享写锁碰撞当作整项验收通过。

## 测试回执：失败保留、子集不累加

以下路径均相对本独立仓：

1. 初始 Python 阶段通过：`.artifacts/selftest/1790073870373676000/result.json`。初始源指纹 `3b2c7c488c5b514aa1226d737796f4d5e2b7a687de06db2d23c4be33ffa8c5d3`。
2. 初始 Rust 新跑370通过/2失败：`.artifacts/checks/toolmcp-a66ef764-rust-regression-1790073984943159000/result.json`。不是沿用旧372全通过。
3. 第一轮源码修复后372通过/1失败，venv解释器前置条件尚未补全：`.artifacts/selftest/1790074936539227000/result.json`。
4. 后一轮源指纹 `01f197fe51c50acddae7b9209ca39795c078c9f8d744238e554627e0605fdbbd`：8个前序阶段通过，full-rust-regression因HashSet.push编译失败，后序未执行：`.artifacts/selftest/1790075251193082000/result.json`，文件SHA `d8f250c7d566524083eb2c53d5525b9833f1d0ab372a52aad6dff025e5aa1fd4`。
5. 当前路径回归7/7：`.artifacts/checks/toolmcp-a66ef764-final-exec-path-1790075892183753000/result.json`；job `5e71f983-12db-4057-822c-4598e827f9b7`；2026-09-22T11:18:12.184121Z至11:18:13.302621Z。
6. 当前输出身份回归1/1：`.artifacts/checks/toolmcp-a66ef764-current-output-identity-1790076209585383000/result.json`；job `4e778cd7-02ce-46dc-b0a0-ad6eaefa99fe`；2026-09-22T11:23:29.585782Z至11:23:30.821519Z。

未取得所有当前提交及dirty合并后的同SHA全套通过，不把上述回归相加为全量PASS。现有Rust警告仍保留。未生成本轮最终签名桌面制品。

## 连接日志和运行身份

安装文件 `/Users/didi/Applications/Coding Tools MCP Personal.app/Contents/MacOS/coding-tools-mcp-personal` 本轮一次成功实读SHA为 `dcc37a568d49d50eb87d902d73442da0d7c917e822a6a98ecf0417a604f065f0`。当前PID映射仍受阻。历史 `.artifacts/session87-activation/final.json` 的SHA为 `d3e64d11f558f790074a38746460166a0b12ad5bb5a3b7cb286b75c7e33e00d3`；它不是本轮实时PID证明。当前客户端真实33定义与Skill/任务调用成功优先于早期Unknown tool；不重装已运行0.3.4。

日志目录：`/Users/didi/Library/Application Support/coding-tools-mcp-personal/logs/d9dc2a3fa7a94f6498a130b7cf32a4e2/`。FRP文件SHA `6f50e68e37dc89ffbd1042fcfcb5b91717813b44f63a76cbd170844ebed8d111`。已核主机和HTTP日志时区+08:00：18:18:20.207 run_id=77d3a03bef1d3ae1报proxy already exists；18:18:22.760 run_id=3a551c07b99e621b恢复成功，分别对应日本19:18:20.207、19:18:22.760。此前存在连接写超时、EOF、session shutdown及无精确时间的自动重连失败记录。

HTTP摘要job `4d54ccbf-6922-4bac-9917-dd8cb82d86fd`，采样2026-09-22T19:11:58.381508+08:00，access快照SHA `e7dd4fcf832b000a18ff90d4fa5a7fb7c3c0e8ad0546ed5790dc3224c62c3a37`：全日11243条；用户快照后3394条后端请求均200。没有后端5xx不代表边缘无502，更不等于工具业务成功。未导出凭据或请求正文。唯一根因仍需服务端FRP/边缘日志和进程归属关联。

## 下一执行边界

先读 cross-requests.md 并确认当前最终写者，接续71ba38e1或更晚提交，重新读取实际源码SHA。不得回退到21c429ce或用旧候选覆盖新修复。原17文件中dispatch/exec已混入他会话修改，只能在归属确认后精确整合。不要另起重复全套争锁。现阶段未完成提交门禁，未尝试本轮git commit/push；不能把历史安全拒绝写成当前已提交。

对新最终SHA执行一次统一回归与构建，再依适用授权精确提交/推送。安装、重启、真实OAuth、分发回退不在本任务新增授权内。若原job状态UNKNOWN/PENDING，先查原job；不可重跑真实业务以确认结果。
