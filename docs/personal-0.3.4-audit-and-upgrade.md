# Personal 0.3.4：近期会话与日志审计、修复和升级

> 2026-09-22 最终复审：补修非法字节与正常 Unicode 混排的分页边界，以及尾部预览
> 误丢孤立续字节的问题。完整回归和重新打包通过；Mac 已安装复审后的 0.3.4，
> 并退出旧进程、重新启动。本地和公网 MCP 均实际返回 0.3.4 / 33 工具。

## 范围与证据

仅修改 coding-tools-mcp-personal 独立 Git，不接管父业务仓。基线提交为
194d5bebf11cba01d9a89c64df699711539119a7。继承 session68 的 UTF-8 输出分页修复，
没有回滚其成果。读取个人版近期历史及实际安装目录的审计库、MCP/FRP 日志。
审计快照截至 Unix 毫秒 1790049741472，前 24 小时共 2,930 条工具记录。
聚合统计不包含请求正文、令牌、口令；原始证据仅留在被 Git 忽略的
`.artifacts/audit-session82/`，不上传用户配置或运行数据库。

高频记录：apply_patch 缺 request_id 32 次，git_status 绝对路径拒绝 30 次，
apply_patch 绝对路径拒绝 18 次，exec 策略拒绝 59 次，原生 ls 参数错误 5 次。
NOT_FOUND、RESOURCE_BUSY、历史游标变化、命令非零退出必须按实际上下文解释，
不能把安全边界、并发保护或业务测试失败一律算作软件缺陷。

## 本次修复

| 问题 | 修复与验证 |
| --- | --- |
| Actions 白名单遗漏 Skill，只有 29 个入口，而 MCP 有 33 个 | 从同一注册表接受 Skill；增加全部工具及输入 Schema 一致性测试 |
| 服务端 33 工具与客户端缓存的 26 工具混淆 | server_info 提供名称/参数指纹；新增无启动副作用的 --personal-tool-contract；缺失字段应刷新客户端，不取消写入保护 |
| 小写关键词设置 case_sensitive=true 仍匹配大写 | 显式保存大小写布尔值，敏感/不敏感均有回归 |
| 子目录 glob 按仓库根路径匹配而漏检 | 同时支持扫描目录相对路径与原有工作区相对路径；结果路径格式不变 |
| 排除子树仍遍历，触发访问预算 | 在 WalkDir 递归前剪枝明确的 /**、/**/*；文件 glob 不误剪目录 |
| 搜索前后文可携带巨大整行 | 前后文沿用 UTF-8 安全的预览字节限制 |
| 原生 ls 忽略请求 workdir | 子目录参数从所请求 cwd 解析；工作区/软链边界保持 |
| durable 默认值与超时策略矛盾 | 按实际非交互 durable 默认值计算上限；tty 与非持久模式仍为原上限 |
| Actions 同步调用阻塞异步执行器 | 有界阻塞线程执行，32 总槽、24 长等待槽；断连不提前释放槽位，旧非受管写操作仍保留串行锁 |
| 中文日志分页/尾部出现截断乱码 | 合并原分页修复，并对尾部预览对齐 UTF-8；真正非法字节不伪装成有效文本 |
| 重装可能覆盖新配置或与另一升级互相覆盖 | 默认只读、精确应用身份、临时暂存、完整应用清单、私有配置备份、并发输入校验、跨进程单写者锁和应用回滚 |

首次行为回归实际出现 7 个失败；补充回归复现日志尾部失败及 Actions/超时两个失败。
Actions 回归修复前，同一执行器的 25ms 计时器约 959ms 才完成；修复后通过
180ms 的断言。这是隔离测试，不是对真实公网延迟或所有并发场景的性能承诺。

## 代码审查与验证

手工核对实际调用者、注册清单、Actions/MCP 参数、持久锁范围、退出状态和回滚路径。
此前 session82 的 GitNexus 和辅助规划工具受执行限制，未冒充图谱审查。
最终复审在当前开发环境通过版本锁定的 Probe CLI 和 GitNexus 1.6.9 补齐证据：
修改前执行影响分析，`tail_file` 标记 HIGH，影响输出采集、任务状态和等待路径。
增量图谱曾发生 FTS 索引错误，随后使用只重建索引的完整分析恢复成功。
ARC-8 漂移核验的结构证据通过；其结论来自提供的源码、差异和运行证据，
不代替 Agent 语义审查或真实测试。
没有新增依赖，没有改变 OAuth、命令白名单、工作区写入边界、补丁哈希/幂等保护。

完整回归入口：

```sh
python3 scripts/selftest_personal.py --suite all
```

14 个阶段覆盖 Python、持久运行时、MCP、策略、Skill、前端与桌面构建。
最终回执应以 `.artifacts/selftest/<本次运行>/result.json` 的 passed 和
source_sha256 == source_sha256_after 为准。主程序 275 项 Rust 测试、24 项 Python
测试（含 11 项升级器测试）、47 项 Skill 定向测试通过；定向子集与全量测试有
重叠，不能累加后宣称独立测试数量。前端检查为零错误、零警告。

本次增加的升级回归涵盖只读计划、完整备份、重复运行、错误身份、暂存失败、
安装后失败回滚、配置并发修改、构建物变化、危险软链和第二写者拒绝。

## 打包与安装：严格区分磁盘文件和运行服务

在本独立仓目录依次执行，每条以真实退出状态为准：

```sh
python3 scripts/run_checked.py --name personal034-bundle --timeout 540 -- npm run tauri -- build --debug --bundles app
python3 scripts/upgrade_personal.py
python3 scripts/upgrade_personal.py --apply
```

安装目标为 `~/Applications/Coding Tools MCP Personal.app`。
使用的是用户当前 `~/Library/Application Support/coding-tools-mcp-personal/data/`
数据，不从旧版或 `.personal-home` 测试数据重新导入，不写活配置，不改端口、
工作区、OAuth 或 FRP。升级器在
`~/Library/Application Support/coding-tools-mcp-personal-backups/<时间戳>/`
保留 `previous.app`、`personal-data/` 和 `upgrade.json`。

成功回执 `installed_app_files` 只表示应用文件替换、配置逐字节一致和原生检查通过。
它明确不表示运行进程已重启、现有服务已换版、远端插件缓存已刷新。
为避免中断其他对话，升级器不停止进程、服务或隧道；运行实例切换必须与实际
空闲窗口配合。安装文件校验可使用：

```sh
"$HOME/Applications/Coding Tools MCP Personal.app/Contents/MacOS/coding-tools-mcp-personal" --personal-check-config
"$HOME/Applications/Coding Tools MCP Personal.app/Contents/MacOS/coding-tools-mcp-personal" --personal-tool-contract
```

随后用当前连接的 server_info 核对运行版。若仍返回 0.3.3，说明现有实例尚未
重新加载；不能因为磁盘显示 0.3.4 就宣称服务已切换。客户端工具清单必须实际
可见 33 项，并包含 apply_patch 的 request_id/expected_hashes、exec 的持久任务
参数后，才可认为桥接一致。服务端无法证明客户端缓存已同步，因此返回 null。
原生输入框 `$` 下拉不在本插件可实现或已验收范围；文本 Skill 别名与原生 UI 不同。

## 回滚与未消除的问题

安装后检查失败会回退本次应用文件，不把旧备份覆盖活数据；若发现他人改过
目标应用则停止自动覆盖，保留 receipt 供核对。需要人工回退时，先核验该次
upgrade.json 和 previous.app，在对应服务空闲并退出后恢复此前应用；默认不要
恢复 personal-data，因为它可能比用户当前配置旧。

FRP 日志中的 EOF、session shutdown、代理重名与连接超时不等于 Rust 代码缺陷；
日志后续已记录重新登录/代理启动成功。本次没有控制远端 FRP、修改认证或承诺
消除运营网络中断。错误路径和无效游标需要按接口约定重新定位；安全拒绝不会
通过扩大权限消除。未进行 Windows/iOS 真机、签名发布或全量公网业务验收。

## 最终复审与运行切换（2026-09-22）

新增两项真实红绿回归：

- 存在非法字节时，以小字节页读取日志仍须保留后续完整中文和 emoji，终态残缺
  序列只形成一个替代字符。分页按有效码点及非法序列分别推进，保持有界前瞻。
- 尾部预览最多额外回看三个字节，仅跳过经验证跨越预览边界的有效码点。
  孤立的 UTF-8 续字节继续保留非法数据证据，不再被无条件丢弃。

先分别复现失败，再修复并运行完整 14 阶段。最终通过回执为
`.artifacts/selftest/1790057368138883000/result.json`，源码前后指纹均为
`d2d13c08a454bc205bd3ce2d0772eccbb5ecef2031b880d6fc4b638bc795f5e8`。
275 项主程序 Rust 测试、24 项 Python 测试、运行时全套测试均通过；47 项 Skill
测试是主程序全量中的定向子集，不重复累加。前端检查零错误、零警告。
macOS debug app 重新构建成功，Rust 构建保留 9 条既有未使用代码警告。

安装后二进制 SHA-256：
`90df3ce685b0f89e3237c822f84d3da77a5d0933ddd6bbc4e97d2eb72d7554bd`。
本次回滚备份位于
`~/Library/Application Support/coding-tools-mcp-personal-backups/20260922T061242Z-1790057562888748000/`。
升级阶段 `profiles.json` 与备份逐字节一致；真实应用重启后发生 JSON 键顺序
重排，但解析后的全部配置值与备份完全相同。工作区、OAuth、端口和 FRP 未更改。

已通过原界面停止旧 MCP、退出 0.3.3 进程并重新启动安装后的应用。
本地 `127.0.0.1:28766/mcp` 与既有公网地址的真实 initialize 和 tools/list
均验证 0.3.4 / 33 工具；补丁 Schema 包含 `request_id`、`expected_hashes`。
真实 search_skills、read_skill、invoke_skill 已调用成功；原有 Actions 保持停止。
安装版通过真实持久命令输出非法字节、中文和 emoji，分别使用 1、2、3、4 字节
分页完整还原。受管补丁在一次性忽略目录夹具上验证创建、幂等重放、过期哈希拒绝
和删除清理；测试文件已移除，未修改业务文件。
服务端工具清单可用不等于 ChatGPT 已打开会话的缓存刷新；后者仍需客户端实际
重新发现确认，本文不将本地协议探针冒充该客户端的验收。

最终私有回执集中在 `.artifacts/final-review-20260922/`；构建回执为
`.artifacts/checks/final-review-app-bundle-1790057506080865000/result.json`。
所有私有配置、日志、运行库、图谱和应用构建产物均不进入 Git 提交。
