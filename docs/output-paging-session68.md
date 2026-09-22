# TOOL-MCP / session68：输出分页 UTF-8 边界修复

记录时间：2026-09-21T16:30:38.924720+00:00。本文件是子仓本轮修复正文；主仓 execution 目录仅保留交接与机器回执引用。

## 基线与范围

独立 Git：`coding-tools-mcp-personal/.git`，分支 `main`，HEAD `194d5bebf11cba01d9a89c64df699711539119a7`。主仓为 `master` / `0c7eaa3078b5ea5739ff62583619e18dd189e846`。开工子仓干净；保留后继性能与自动 Skill 提交，不把会话59中的旧 ahead 数字覆盖本轮观察到的 ahead=0/behind=0。此次不创建分支或 worktree，不提交、推送、安装、部署、重启连接或访问生产。

仅改 `personal-runtime/src/jobs.rs` 的 `Store::job_output`，新增 `personal-runtime/tests/output_paging.rs`。无数据库结构、配置、权限、队列、Worker 生命周期、Skill 发现、前端、依赖或版本号变更。

## 真正复现的缺陷

原实现按任意字节 limit 切片后调用 `String::from_utf8_lossy`，有效中文或 emoji 被切开时变成替换字符，且游标已经消费这些字节。当前连接的真实只读调用也复现：自有短作业 `ae4a74b7-a3f5-476d-8532-a21aeed44498` 输出 `中😀A\n`，完整读取为9字节；`read_output(offset=0,limit=1)` 返回 `�` 并把游标推进到1。记录见 `.artifacts/tool-mcp-session68/current-connection-observation.json`；这是本轮工具响应字段摘录，不伪称网络原始包。

预修复新增8项测试：1通过、7失败，Cargo真实退出101。失败不仅包含新增元数据字段，也直接包含中文、活跃残缺字符和 valid-prefix 输出内容断言。外层证据采集命令退出0仅表示回执保存完成，不代表失败测试通过。

## 最小改动与读取合同

每次最多读取 clamp(limit,1,1048576)+3 字节，并限制在一次文件长度快照内；3字节仅用于补齐 UTF-8 边界，不扩大8MiB保留日志上限。优先返回完整字符；当 limit 为1—3且放不下首个完整字符时，返回一个完整字符，最多多3字节。普通页不超过请求上限。

`bytes_read` 是本页实际消费的源字节数，不包含前瞻；`poll_offset` 按消费字节推进。调用方始终使用返回游标，不能按显示字符数计算。

活跃或结果未知的作业如只剩半个字符，返回空内容、`pending_utf8_bytes>0`、原 `poll_offset`、`next_offset=null`。这表示当前不能继续立即分页，不代表任务完成；后续按原 job 查询状态和 poll_offset，不重放原命令。残缺前有完整前缀时先返回前缀。

非 UTF-8 原始字节、终态残缺字节保持有界有损展示并明确 `content_lossy=true`；不承诺二进制无损恢复。随意指定到字符中间的 offset 也不保证无损，应使用服务器返回的 offset。没有增加任何自动任务重试、资源释放或权限降级。

## 实际测试与构建

最终命令（子仓目录运行）：

```sh
cargo test --offline --locked --manifest-path personal-runtime/Cargo.toml --target-dir .artifacts/tool-mcp-session68/cargo --config build.jobs=2 --lib --test output_paging --test state --test performance -- --test-threads=2 --nocapture
cargo build --offline --locked --manifest-path personal-runtime/Cargo.toml --target-dir .artifacts/tool-mcp-session68/cargo --config build.jobs=2 --bin coding-tools-personal-worker
```

29个唯一测试用例通过：既有库单测2、新增分页8、元数据并发/等待5、任务/配置/幂等状态14。新增8项动态记录3017次断言；其他21项未做动态断言计数，不把3017说成整个项目的断言总数。初次2项基线已含在最终29项内，不重复加总。所有测试调用真实Rust库与隔离SQLite/日志；状态重开、人工unknown夹具不等于操作系统重启或真实断网恢复。

8线程元数据压测：80请求，每次50任务，P50 3.124ms，P95 5.342ms，wall 35.461ms。该数值是本轮本机夹具观测，不与旧机器负载下历史数值冒充严格A/B，不代表ChatGPT/FRP端到端性能。

定向测试含编译总墙钟 5.485s；子进程用户CPU 1.584s、系统CPU 1.367s；macOS RUSAGE_CHILDREN最大子进程RSS 195264512字节（包含编译器，不是正在运行MCP的内存）。构建退出0，制品 `.artifacts/tool-mcp-session68/cargo/debug/coding-tools-personal-worker`，SHA256 `6a7379ae3828703a27b9d2f28a7e4f99032a369b4b7f1010326d0e7ca4bac7cb`。这只是独立worker制品，不是Tauri桌面安装包，也不会使当前连接的库代码自动生效。

原始日志和每阶段命令/输入/输出SHA、资源、退出码在 `.artifacts/tool-mcp-session68/{red,green,build-review}/`。源码输入在测试、构建前后无漂移；私有profile文件在记录区间内SHA保持 `55de938f2b76c4c055f3a1818ea3f9b0a10494f6338e978a1defd02fe35b2164`，未输出其内容。

## 实际差异审查

基于65行Git差异、原方法及调用者审查。调用范围是 `dispatch.rs:read_output → personal.rs:job_output → Store::job_output`。`git diff --check`退出0。前瞻始终有界，原游标只消费已返回内容，EOF不重复、非法字节有标记、活跃残缺不造成next_cursor空转。读路径不新增命令、任务重放或外部副作用。

Probe CLI确实执行了code_insight，但结果为 `local_fallback_no_call_graph` / `impactAnalysis=false`；本轮使用受限源码检索补足调用者核对。不声称GitNexus图级影响分析或独立第二模型审查通过。未更改现有符号的其他调用契约类型。

## Skill、工具暴露与未验项

实际连接server_info报告0.3.3/33工具，而本会话只有26个schema；task_*三项和Skill四项不可调用。exec参数缺mode/resources/request_id；apply_patch缺request_id/expected_hashes。源码personal_schema已声明这些字段，不能通过移除哈希或幂等保护来适配旧连接。缺失schema工具未被伪造调用；本轮源码编辑使用已授权受管排他exec、HEAD/文件SHA核对和原子替换。

按实际目录搜索后，webapp-testing与playwright正文经授权文件读取加载；doc-coauthoring在本轮检索的目录中未找到。没有页面改动，不运行无关页面截图，不把Skill正文读取当作浏览器验收。$仅为文本别名，不是原生选择器。

完整selftest_personal.py --suite all调用被安全层拦截且未执行；未改包装或通道重跑。本轮只执行了独立、范围缩小的库级夹具回归和worker构建。真实worker取消/超时/进程故障恢复、整套Tauri/MCP/Skill回归、桌面打包、Windows、浏览器端到端仍未重验；会话59的260项Rust/35项runtime等旧回执只作为明确日期的历史结果。

运行进程定位pgrep被白名单拒绝，未换方法探查；实际运行制品路径/SHA未证实。当前MCP响应正常不等于候选已安装，且真实1字节分页仍复现旧行为。安装与连接器schema刷新须分别按授权和会话保护窗口推进。

## 输入与回滚边界

jobs.rs：`3b6adde2021f9861f4d2f3472ce943578f5766c774cd4855301f5d01971a006b` → `f591dd774e7f87ffd75df28910a4a2a5322a7005839fb2051e8047bd7708148d`；新测试SHA `621aeae6776028790aa83098fcab295d51936b29d5bca5aa20f40886ebb50117`。原字节备份和精确diff分别在 `.artifacts/tool-mcp-session68/jobs.rs.before`、`.artifacts/tool-mcp-session68/build-review/source.diff`。

未执行回滚。未来撤回时只处理本轮精确diff，先核当前SHA；发现后继改动必须重新合并，不用git reset/checkout整仓或覆盖旧快照。报告与源码不得和主仓其他历史/业务文件混合提交。
