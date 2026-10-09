# GPTBridge · dot 使用与兼容说明

适用源码：0.4.2，增量基线 `17f47d3d261aa23f9e8d2c11d269438e000bceba`。本文说明现有工具如何直接操作工作区；安装版本、监听服务及真实客户端验收须分别核对。用户提供的设计稿是需求来源，其中的模拟任务和连接状态不属于运行证据。

## 先确认连接与项目

客户端允许且连接器可用时，dot 与原有 `@GPTBridge` 使用同一个 `/mcp`、既有认证和工具。无需为 dot 新建执行器或迁移项目。客户端是否选中连接器、显示名称、权限确认和计费由对应客户端决定，GPTBridge 不替它们作保证。

单工作区先调用 `server_info`、`get_default_cwd`，核对项目、运行版本、工具档位和权限。共享网关先调用 `workspace_list`，再将返回的 `workspace_id` 放入每次业务调用，包括任务恢复、命令轮询和输出读取。工作区必须由本地主人登记，远程不能把任意路径加入网关。

`server_info.direct_workspace` 是可忽略的说明字段：`execution_path=native_tools`、`external_agent_required=false`，持久记账继续使用原 task/request 生命周期。实际可调用工具以 `tools/list` 为准；该元数据不代表 `dot_supported=true`，也不改变权限。

## 在宿主应用观察

打开项目工作区详情的 **dot · 原生工作区**，同页查看“同一连接，直接操作”“连接诊断”“任务与恢复回执”。

- “刷新观察”读取真实连接和该工作区最近的有界任务数据，不执行命令。
- “复制使用提示”“复制接续指令”只复制文本，由已连接客户端继续处理。
- 任务状态、作业状态和检查点来自现有后端；没有作业时不能据任务 active 推断正在运行。
- 端口可达、客户端目录已发现、平台授权允许、命令结束和结果已取回分别判断。客户端缓存和 dot 路由无法从宿主探测确认，界面显示未验证。

## 单工作区的最小操作闭环

以下 JSON 块是工具 `arguments`，不是自动执行脚本。占位 ID、哈希和输出引用必须替换成前一步真实返回值；在临时测试工作区中准备 `example.txt`，初始内容为 `before`。生产或已有仓库需要遵守其实际授权和测试要求。

1. 创建或恢复任务。新任务提供稳定 `request_id`，恢复时使用原 `task_id`，无需重复初始化模板：

```json
{"goal":"在临时工作区验证文件修改和命令输出","request_id":"dot-example-open-1"}
```

调用工具：`task_open`。保留返回的 `task_id` 和 `revision`。`task_open`、`task_status`、`task_checkpoint` 是持久记账与恢复工具；原生文件和命令工具不要求外部 Agent。

2. 调用 `read_file`，保留返回的整个文件 `file_sha256`，即使只读取一段文本也不能自算该片段哈希代替它：

```json
{"path":"example.txt","max_bytes":8192}
```

3. 调用 `apply_patch`，携带真实 task ID、稳定请求 ID及读取的哈希：

```json
{
  "task_id":"TASK_ID_FROM_TASK_OPEN",
  "request_id":"dot-example-patch-1",
  "expected_hashes":{"example.txt":"FILE_SHA256_FROM_READ_FILE"},
  "patch":"*** Begin Patch\n*** Update File: example.txt\n@@\n-before\n+after\n*** End Patch"
}
```

新文件的哈希前置条件使用 `null`，仅适用于文件确实不存在。旧哈希冲突返回 `STALE_FILE` 时重读、合并当前改动，并为新的补丁内容使用新请求 ID。不能恢复旧全文覆盖其他作者。相同 ID、相同载荷用于查询原回执；同 ID 改变载荷会发生幂等冲突。

4. 调用 `exec_command` 校验真实文件。示例需工作区命令策略允许已安装的 Python：

```json
{
  "task_id":"TASK_ID_FROM_TASK_OPEN",
  "request_id":"dot-example-command-1",
  "cmd":"python3 -c \"from pathlib import Path; assert Path('example.txt').read_text().strip() == 'after'; print('verified')\"",
  "workdir":".",
  "durable":true,
  "tty":false,
  "mode":"read",
  "yield_time_ms":0,
  "timeout_ms":30000,
  "max_output_bytes":8192
}
```

这里 `mode=read` 对应只读取的校验；实际写源码命令须使用 `write`，构建按真实行为使用 `build`，不能为了并行把写操作声明为 read。`cmd` 是单个直接命令，不是外层 shell 链；解释器允许范围继续由原策略决定。

5. 返回 queued/running 后调用 `write_stdin` 查询原 `session_id`，不重启命令；持久命令关闭 stdin，轮询的 `chars` 留空：

```json
{"session_id":"SESSION_ID_FROM_EXEC_COMMAND","yield_time_ms":1000,"max_output_bytes":8192}
```

也可用 `task_status` 的 `job_id` 查询原 job。需要完整输出时调用 `read_output`，使用返回的 `output_refs.stdout` 或 `output_refs.stderr`：

```json
{"output_ref":"OUTPUT_REF_FROM_EXEC_COMMAND","offset":0,"limit":4096}
```

沿返回的 `next_offset` 继续，保持同一流的引用。`read_output` 和 `write_stdin` 的单工作区公开 schema 不接受 `task_id`；归属由原句柄决定，不能附加未公开字段。共享网关仍要额外传 `workspace_id`。

6. 核实 `status=exited`、`exit_code=0`、`command_ok=true`，检查输出截断、缺失或损失标记，再调用 `task_checkpoint`：

```json
{
  "task_id":"TASK_ID_FROM_TASK_OPEN",
  "request_id":"dot-example-checkpoint-1",
  "expected_revision":0,
  "state":"active",
  "checkpoint":{
    "summary":"已验证临时文件补丁与命令输出",
    "next_step":"核对客户端目录和调用证据",
    "steps":{
      "read-patch-run":{"state":"passed","evidence":"真实校验日志的已授权路径或回执引用"}
    }
  }
}
```

`expected_revision=0` 只适用于步骤 1 实际返回 0 且尚无其他检查点的情况。后续始终使用最新 revision；过期时重新读取，不覆盖较新的进度。工具接受证据字段不等于自动证明业务正确，声明通过前检查引用的真实文件和结果。完成任务还需满足全部已声明验收项并无未解决 job，不能因为保存了检查点就标记完成。

## 结果不明时怎样继续

| 实际状态 | 操作 |
| --- | --- |
| queued / running | 继续查原 job/session，使用有界等待，不再次启动同一命令 |
| exited，非零退出码 | 阅读 stdout/stderr，按实际原因修复后创建新的逻辑尝试 |
| timeout / queue_timeout / cancelled / spawn_failed | 保留原回执，检查是否已产生外部效果，再决定后续步骤 |
| unknown / 连接超时 / 响应丢失 | 查询原 task/job/request，并核实文件或外部效果；不盲目重放 |
| 输出有 next_offset 或截断标记 | 按引用分页取结果，不能把部分输出宣称为完整输出 |
| 任务已 completed | 原请求可恢复原回执；新的工作需要后继任务 |

worker 或主机崩溃并不保证任意进程续跑。恢复使用当前工作区和原持久记录，不恢复整仓旧快照；任务数据格式和保留规则沿用既有配置。

## 安全与读取范围

| 维度 | 当前行为 |
| --- | --- |
| 单工作区原生读取 | 旧逻辑允许显式外部绝对路径或 `..`；正常相对路径的软链接逃逸会拒绝 |
| 共享网关原生读取 | canonical 后必须位于选定真实根目录，拒绝跨仓库路径与逃逸 |
| `direct_workspace.security.read_scope` | 严格读取为 `workspace_root`，旧读取为 `explicit_external_paths_allowed`，从真实 strict-read 标志映射 |
| 补丁和命令工作目录 | 继续受已有路径、工具档位、策略与合作锁约束 |
| 命令进程隔离 | `execution_isolation=policy_only`，`sandbox_enforced=false`，当前没有操作系统级文件系统沙箱 |
| 权限拒绝 | 保留原拒绝机制，不用说明文字或另一个工具扩大授权 |

只读权限不自动收紧单工作区读取根目录。read/build/write 用于合作调度与资源锁，不限制子进程全部文件系统访问；配置解释器时需按其实际能力判断。

## 更新后如何核对目录

1. 保留升级前运行版本、模式、工具档位和脱敏目录基线。
2. 更新服务后重新 initialize、`tools/list`、`server_info`，核对运行版本和说明。
3. 工具名称、完整 `inputSchema`、annotations、暴露层级及旧响应关键字段应保持原契约；说明文本与新增可选元数据单独审查。`tool_contract.schema_sha256` 覆盖名称与 inputSchema，顶层说明和 annotations 需要另外比对。
4. 使用客户端支持的刷新或重新初始化；必要时新建会话。先记录实际流程，不假定重启服务已刷新客户端。
5. 原有 `@GPTBridge` 与 dot 各自实际运行读→补丁→命令→原句柄输出，保留脱敏工具轨迹。缺少可用客户端时该项记录 `NOT_VERIFIED`，不能用服务端测试替代。

当前 initialize 为 `tools.listChanged=false`；带 `Accept: text/event-stream` 的 `/mcp` GET 返回 `405`。普通 GET 的健康 JSON 不属于 SSE 通知，当前没有工具目录推送更新功能。

## 验证与回滚

源码/协议、构建制品、安装路径、运行进程、HTTP、宿主 UI、客户端调用分别记录。精确验收项见 [需求](specs/dot-compatibility/requirements.md)、[设计](specs/dot-compatibility/design.md)、[执行清单](specs/dot-compatibility/tasks.md)；本轮结果及截图存于 `docs/gptbridge/dot-compatibility/`，清单本身不代表全部通过。

替换应用前保留旧 App 完整副本、版本和 SHA，确认活动命令可安全延续。回退时安全退出新应用并恢复旧 App，读回安装字节、进程与服务；保留任务库和恢复句柄，不回滚用户工作树。客户端目录缓存仍可能需要其支持的刷新。

源码定位：[`registry.rs`](../src-tauri/src/tools/registry.rs)、[`personal_schema.rs`](../src-tauri/src/tools/personal_schema.rs)、[`personal.rs`](../src-tauri/src/tools/personal.rs)、[`dispatch.rs`](../src-tauri/src/tools/dispatch.rs)、[`workspace.rs`](../src-tauri/src/tools/workspace.rs)、[`gateway.rs`](../src-tauri/src/mcp/gateway.rs)、[`server.rs`](../src-tauri/src/mcp/server.rs)、[`listener.rs`](../src-tauri/src/mcp/listener.rs)、[`jobs.rs`](../personal-runtime/src/jobs.rs)。这些路径是源码依据，不能替代运行实例或客户端验证。
