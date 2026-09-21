# 个人版设计

## 真实调用链与影响面
MCP server::handle_tools_call / Actions -> tools::dispatch::call_tool -> policy -> patch/exec/history。
context.rs 持有单 workspace Harness；dispatch 当前从 current_task 取单活动任务，并把 ok=true（包括 running）登记为 completed。patch.rs 当前没有 SHA 前置条件或跨进程提交锁。exec.rs 的 SessionStore 仅内存，命令完成会移除输出索引。ChatGptSessionPrompt.svelte 直接要求复制长提示词。platform/*/mod.rs 共用旧应用数据目录，DataStore::load 首次可能迁移原配置。

## 实现
FR-1/3：增加 personal-runtime 小型 Rust 库与 task 工具，SQLite 存任务、绑定、步骤、事件和操作回执；有界返回。显式 task_id 优先，可信传输会话元数据仅用于定位；不把别的任务自动选作当前。
FR-2：受管源码修改统一源写锁；补丁读取、SHA 校验和提交在同一锁内；小补丁串行，分析并行。多文件异常保留未确定状态，不回滚他人文件。
FR-4/5：复用原命令策略与解析；持久命令保存唯一请求 ID 后启动同版本 worker；worker 独占自身存活锁，持有运行名额/资源锁，落盘输出和终态；状态未知禁止自动重放。
FR-6：个人版 app ID、数据根与进程标识分离；配置导入纯函数和明确入口；实际服务不启动。
FR-7：纯 runtime 测试与集成工具测试先在临时根运行，再做 Rust/Svelte 检查。所有脚本提供确定退出码与原日志。

## 风险
影响核心 dispatcher、patch、exec、平台配置路径，视为高风险集成；先模块测试，再完整编译回归。GitNexus 若不可用，以本次实际源码调用链为准，不能把历史图谱当当前结果。跨进程文件锁是协作机制，不管控外部编辑器和未申报写入程序；遇到冲突必须重新读取并重验。
