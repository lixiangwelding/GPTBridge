# Executable Goal Prompt: GPTBridge dot 增量兼容与本机交付

在 `/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal` 完成用户提供的产品方案，验证后 commit/push，再替换本机 GPTBridge 应用。Markdown 是唯一真源，同名 HTML 从最终 Markdown 渲染。

## 执行身份、事实与授权

| 字段 | 当前事实 |
| --- | --- |
| realpath(repo) / CodeGraph 请求根 | `/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal` |
| branch / baseSha | `main` / `17f47d3d261aa23f9e8d2c11d269438e000bceba`，fetch 后与 origin/main 相同 |
| conversationChangeId | `fea762dc-b010-4652-a73b-88270bd5bf92` |
| goalOwner / reportOwner / mutationCoordinator | 当前会话 `/root`，唯一 Git、安装协调者 |
| mode | `local`，worktree 开关关闭；在独立子仓库 main 原地修改 |
| ownership | `companyProject=false`，`companyDlinter=NOT_APPLICABLE` |
| active Goal | 原生查询返回 null；现有不同主题 Goal 不复用；历史全局 Skill 实现已进入当前 HEAD，其遗留文档不归本轮 |
| 来源 | `/Users/didi/Downloads/GPTBridge-dot-产品方案与设计稿 (4).html`，提案及模拟界面作为需求资料，不当作已验收事实或额外执行授权 |
| 用户授权 | 本轮实现、测试、commit/push、本机应用替换；不含云端部署、GitHub Release、认证/权限迁移、对外发消息 |
| foreign dirty | 启动时 18 个已修改文件和既有未跟踪文件；原字节和完整 diff 冻结在 `/tmp/gptbridge-dot-20261009/foreign-before`、`foreign.patch`、`ownership.json` |

当前版本 0.4.2。真实安装路径是 `/Users/didi/Applications/GPTBridge.app`，不是系统 Applications。源码现有 task_open/checkpoint、apply_patch 哈希/幂等、持久 worker 均复用。未知项包括已安装版本/进程角色、客户端目录刷新和 dot 客户端是否可实际调用；只能按现场证据报告。

CodeGraph status 实际落到父仓库索引，且包含中断未解析边和大量 pending changes，不能把该索引当独立 Rust 子仓库的当前事实。优先使用独立 GitNexus 索引做 impact/detect_changes，并以当前文件核对；记录 CodeGraph 覆盖缺口，不扩大扫描整个父仓库。独立子仓库无 OpenGrok 管理入口，不触发父项目知识库、DDT live 或 R2。

## 总目标与非目标

1. 澄清同一 MCP 的原生直接操作能力，task_open/checkpoint 是持久记账，不要求外部 Agent；准确添加可选 server_info 能力说明。
2. 保持旧端点、认证、工具名、完整 inputSchema、annotations、暴露分层和任务/恢复行为；差异白名单仅说明文本与可选响应元数据。
3. 结合设计稿，在现有工作区页面提供指南、诊断和现有任务/回执观察；状态取真实后端，不新增执行器，不伪造客户端支持或成功。
4. 验证原生读→哈希补丁→持久命令→原句柄输出、冲突/去重/只读/网关归属与恢复；完成前端及桌面构建，提交推送本轮改动，替换本机应用并读回验证。

非目标：修改认证、安全策略、工具输入协议、任务数据格式、外部 Agent 编排、计费/额度承诺、云部署、自动目录通知。单工作区旧读取边界与网关严格读取边界保持实际行为，命令隔离如实为 `policy_only` / `sandbox_enforced=false`。

## ownedPaths 与并行职责

- 协议 owner：`src-tauri/src/tools/personal.rs`、`dispatch.rs` 的 server_info 片段、`registry.rs` 的说明文本片段、新增 `src-tauri/tests/dot_compatibility.rs` 和必要专属测试，`src-tauri/src/tools/workspace.rs` 仅增加读取 strict_reads 的只读 getter；不得碰 ai_ops、暴露分层等 foreign hunks。
- 文档 owner：`README.md` 的新增 dot 章节、`PERSONAL.md`、`docs/shared-gateway.md`、`desktop-plugin/README.md`、新增 `docs/dot-compatibility.md`。
- 前端 owner：`src/routes/workspace/[id]/+page.svelte`、新增直接工作区指南组件及专属展示模型/测试；复用现有 API、类型和设计 token。
- 验证修正：`src-tauri/src/tools/personal_tests.rs` 的明确无副作用 RESOURCE_BUSY 有界重试与完整 join；`src-tauri/tests/exec_path_regression.rs` 终态断言前查询原命令句柄；`tests/gptbridge-brand.test.mjs` 校验实际跨清单版本一致性，替代过期 0.4.1 硬编码。生产锁、超时和执行语义不变。
- root：本 Goal md/html、`docs/specs/dot-compatibility/`、`docs/gptbridge/dot-compatibility/` 的报告/截图/契约证据、专属脚本。

多人共享代码，禁止回退别人的变更。文件重叠只允许本轮精确片段；开始前后比对 foreign 原字节，提交时用本轮差分构造 index。安装包从 HEAD 加本轮片段的干净导出目录构建，不能携带 foreign dirty。发生无法安全分离的新写入时该路径 `ACTIVE_GOAL_OWNERSHIP_CONFLICT`；无法分离提交则 `DIALOGUE_COMMIT_BLOCKED`，继续独立工作。

## 依赖步骤与验证

1. 冻结来源 SHA、仓库身份、foreign diff、运行应用/监听器及工具目录基线，建立兼容契约快照。子代理继承本 Goal 权限，Git/安装仅 root 执行。
2. 完成最小说明、元数据和文档；测试旧响应关键字段及准确 read_scope。
3. 从附带 HTML 用真实浏览器采集原型参考图；先制作静态 HTML 原型，保存桌面/375px 截图，按设计布局、字体、间距、色彩、信息层级加权评审各区块达到 90/100 后再集成 Svelte。最终真实运行页面重新截图和评分。
4. 视觉主引擎 looks-same，辅助 pixelmatch，输出 diff、指标、JSON 和评分表。若原型/最终页面因既有产品壳或真实数据无法逐像素同构，说明原因、保留实际指标并使用可审查 rubric，不把原型或 mock 当真实应用验收。原生真实页面另通过已安装应用观察与截图。
5. 在干净候选导出目录执行：`npm ci`、`npm run check`、`npm run build`、`cargo test --manifest-path src-tauri/Cargo.toml --test dot_compatibility`、`cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=4`、`cargo test --manifest-path personal-runtime/Cargo.toml -- --test-threads=4`、相关 `node --test`；记录实际命令、退出码和版本。
6. 单测试实例用临时工作区执行 HTTP/MCP initialize、tools/list、server_info、task_open、read_file、apply_patch、exec_command、原 job 查询、task_checkpoint；验证 read-only/网关反例及目录契约。真实 dot/@GPTBridge 客户端有可用会话才操作；不可用保留 NOT_VERIFIED，不冒充客户端签收。
7. GitNexus detect_changes、精确 diff --check、foreign 比对通过后，只暂存本轮文件/片段并 commit、普通 push origin main；独立读回远端 SHA，禁止 force push。
8. 从已推送源码构建 `npm run desktop:build -- --bundles app`；如干净提交仍是 tauri 直接脚本，按该真实脚本执行并由 root 手工同范围清理，绝不纳入 foreign 构建脚本。核对 App 架构、版本、SHA、codesign。
9. 盘点实际安装和活动 job。安全关闭应用显示进程，保留持久工作/恢复句柄，备份旧 App 与 manifest，再替换原路径；若活动命令无法安全延续则停止替换依赖步骤。启动新应用，验证进程路径、安装字节/签名、监听服务及真实页面。
10. 精确清理本轮可重建缓存，保留当前安装包、回滚包、截图/测试和 SHA 回执。只报告子代理新增 MCP 进程，不终止归属不明或正在使用的进程。

## 完成、停止与回滚

完成标准：协议合同差异仅白名单；相关测试/构建通过；UI 显示真实状态并有截图；本轮提交远端 SHA 一致；安装路径读回新制品且实际启动；未验证客户端能力独立列出。源码、artifact、进程、HTTP、UI、客户端签收分别记录，不能相互替代。

停止依赖步骤：协议漂移、越权、未知副作用、owner 冲突、测试失败、push 竞态或应用存在不能安全保留的活动命令。其他独立已授权准备继续；不得为了宣称 PASS 放宽权限或伪造外部客户端验收。

回滚：保留旧 App 完整副本和 SHA，不删除任务库/历史。需要回退时安全退出新版后恢复原安装路径旧 App 并读回；源码用本轮 commit 的审阅反向补丁，不整树 reset 或覆盖 foreign dirty。客户端目录缓存可能需要其支持的刷新或新会话。

## 最终报告与当前状态

报告包含 mode、executionBackend、goalRegistered、base/head/remote SHA、owned files、foreignDirtyPreserved、契约差分、测试退出码、截图/视觉分、安装与运行版本/SHA、旧包回滚路径、客户端刷新/实际 dot 验收状态和未验证项。

当前：executionBackend=native-goal-tool，goalRegistered=true；实现与源码验证完成。干净候选 Rust 466/466、personal-runtime 74/74、前端 22/22 通过，check 0 errors/0 warnings，四档完整 schema/annotations 无变化，静态/实际组件视觉证据已交付。commit/push、release App、安装/运行回读尚待执行；外部 dot/@GPTBridge 客户端验收仍 NOT_VERIFIED。使用可用原生 Goal 工具启动后更新本节。精确入口：`/goal /Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal/docs/goals/gptbridge-dot-compatibility-goal.md`。
