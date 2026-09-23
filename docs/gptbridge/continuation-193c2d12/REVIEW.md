# GPTBridge 0.4.1 候选收敛与审查

执行任务：`193c2d12-c78a-4a65-a6e8-49947cb4c747`。基线：`ecc4452a1bb4a91497f8f24e9baeda3831b683e2`，独立仓 `main`。

## 本阶段范围

接续用户已批准的 GPTBridge 品牌/插件和受控 Skill 写根候选，不重新开发已提交的 TaskDock 架构，也不重复提交 a5d0d8b 的四个工具。保留旧 bundle ID、二进制名、IPC、任务与配置路径。当前第五个“MCP 连接”入口已有实现，本轮同步中英文 README；旧四页截图明确是早于新入口的历史前端预览，不晋升为最终界面或原生验收。

本輪另外补回 Skill 写根合并时丢失的 `check_command`、`tool_catalog_check`、`read_files`、`stat_path` 四个目录断言。没有改变它们的运行参数合同。

## 逐项审查

| 范围 | 结论与证据 |
| --- | --- |
| 授权来源 | `workspace/model.rs` 默认空根列表，canonical 路径、根/Home/重复目录校验；`commands/workspace.rs` 仅本地可信保存，运行中的 MCP 不接受授权集合变化。发现 Skill 不等于自动批准写入。 |
| 路径与参数 | 已完整读取 `tools/skill_write.rs`：只接批准 root_id，参数类型/未知字段/完整哈希校验；目标使用受限 Workspace，拒绝绝对路径、父目录逃逸与软链接。 |
| 事务和并发 | 已逐段读取 `personal_patch.rs` 与 `patch.rs`：复用原补丁事务、请求幂等、写前后 SHA；同时取得 canonical-root 和本工作区锁，跨工作区写相同全局根具备互斥测试。 |
| MCP/Actions | `registry.rs`、`policy.rs`、`dispatch.rs` 与合同测试：core39工具；只读档位不开放写Skill；Actions只允许枚举，不开放全局Skill写入。MCP会话元数据仅注入历史/任务入口，不污染Skill补丁的严格参数集合。 |
| 界面 | `SkillWriteRootsForm.svelte` 通过真实Svelte编译合同；保存只更新选定授权，运行时禁用，无自动重启。项目切换和连接入口沿用现有Tauri工作区。 |
| 插件 | 原registered app ID不变；新插件名GPTBridgePlugin；registered/stdio不重复加载；应用/插件版本不匹配会拒绝。Windows原生插件安装尚不在本阶段验收内。 |
| 精确交付 | `release.py` 只列明品牌/Skill候选及其文档、历史截图和本阶段证据；先校验源SHA/HEAD/空索引，再逐文件暂存与逐blob校验。未跟踪构建缓存、其他交接、FRP、AI-OPS不进入提交。 |

## 实际验证

正式同源结果：`.artifacts/continuation-193c2d12/verify-1790172574847283000/result.json`。284个源码/配置/测试文件前后指纹一致。对应逐文件SHA与正式结果作为 `qualified-source.json`、`qualified-results.json` 附在本目录。

- 完整Rust：459通过，0失败/忽略；含库、main及全部集成套件，不重复累计先前346库测试。
- Python个人工具：67通过；品牌与写根界面合同：10通过。
- Svelte检查、Vite生产构建、显式worker构建均成功。
- 图谱：`list_tools_for_profile` 上游 HIGH，27影响点、10直接调用点、3模块。来源 `job:1fe0d969-4f4f-4a8e-ae00-a0e5c9b6e5b4:stdout`。索引包含旧测试名，按辅助证据使用，不冒充完整最新源码图谱。
- `code_review` 已实际执行，返回指导、真实Git差异及未跟踪文件警示；并不自动证明无缺陷。本表为逐段实际源码审阅结论。
- ARC-8 validate 已补足事实、不变量、候选比较、边界和过渡依据并返回本阶段PASS：`job:1bb0043a-fec5-46c7-b298-083e716b246d:stdout`。首次空架构工作表的gaps仍保留于旧job，未修改失败记录。本阶段PASS不代表安装或生产权限已启用。

## 未关闭的运行与业务边界

本轮出现一次命令创建前 `STATE_DB: disk I/O error`；后续原任务读取和独立只读命令恢复，磁盘当时可用2929025024字节。未确认根因，不宣称永久修好或已确诊内存泄漏。没有通过更换解释器或通道绕过权限拒绝，没有强制解锁、删除他人缓存或重启服务。

本阶段不安装替换、不改变真实授权根、不改写全局Skill，不执行生产、投稿或收费生成。完整客户端目录同步、运行二进制来源SHA、原生UI与设备验收仍须单独取得。AI-OPS有活跃执行链，本轮不覆盖其源文件；工具重叠文件在本阶段精确提交后以新SHA交接。

语义审查未发现本候选源码提交的阻断性缺陷；真实运行/设备/业务门禁仍未关闭。提交批准必须另绑定实际staged tree和提交前detect-changes结果，本文本身不是commit/push成功回执。
