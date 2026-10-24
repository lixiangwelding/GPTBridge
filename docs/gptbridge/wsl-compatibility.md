# Windows GPTBridge 的 WSL 工作区

本次修复使 Windows 客户端在选中 WSL UNC 仓库时，将文件来源、命令宿主和技能用户保持一致。源码候选状态与已安装版本分开记录；本文不表示已经发布或经 ChatGPT 插件验收。

## 路径与执行

- 支持 `\\wsl.localhost\Ubuntu-24.04\home\用户\仓库`、`\\wsl$\发行版\...`，以及 Windows canonical extended UNC。保留原 UNC provider，避免别名转换造成 canonical scope 不匹配。
- 发行版从已选工作区根解析，不使用默认发行版，也不修改 Windows 全局 WSL 设置。
- 文件工具读取 `/home/...` 时转换为同发行版 UNC 后再走既有 canonical、敏感文件、symlink 与 strict-read 检查。写工具继续使用仓内相对路径，不因兼容 Linux 绝对读取而放宽写入范围。
- 命令 cwd 可用仓内相对路径或同仓 Linux 绝对路径，其他发行版、越界和 symlink 逃逸仍拒绝。
- WSL 工作区命令使用 POSIX 引号解析；策略与进程启动共用同一解析器。普通 Windows 工作区保留 CRT 引号规则，macOS/Linux 不变。
- OS runner 从 `SystemRoot/System32/wsl.exe` 解析，避免工作区中的同名程序。命令以 argv 经 `--distribution`、`--cd` 进入对应 WSL；参数作为固定 Bash 脚本的 `$@` 传递，不拼成 shell 命令。
- 使用用户登录环境，并在存在时加载当前 Linux 用户已有 NVM default，修复非交互登录找不到 Node/npm。不会在 Windows 新装 Node、Java 或 Rust，不修改用户 shell 配置。
- 常规命令可直接传 `git status --short`、`npm test`、`./mvnw test`；无需再嵌套 `wsl.exe`。命令权限和读写门禁继续按原策略检查。

## 技能与共享规则

`/home/<user>/...` 工作区发现同用户的 `.agents/skills` 与 `.codex/skills`，不混用 Windows 用户技能。`/mnt/...` 或 `/root/...` 不猜用户；额外只读技能根继续使用现有 owner 配置。Linux 绝对共享规则路径经文件工具按同发行版转换，文件存在不代表已实际读取。

## 超时与取消

每次实际执行分配独立随机 identity，Linux 中以 setsid/timeout 启动，并设置与请求一致的截止时间。durable worker 和 legacy session 都保留同一 cleanup plan，取消、超时或正常收尾时，按本次 identity 核验并结束自建 Linux 子进程，之后再结束 Windows relay/释放资源。

清理只检查本次随机 identity，不按程序名或年龄结束其他进程，不调用 `wsl --shutdown`。Linux 独立 deadline 在 Windows relay 消失时仍有效。清理失败返回失败/UNKNOWN，不可把原请求当作未发生再重放。

依赖：对应 WSL 发行版已安装 Bash、coreutils timeout/setsid，以及 `/usr/bin/python3`（仅用于归属核验与清理）。缺依赖时应修复该发行版工具链，不退回 Windows 执行冒充成功。主动清除继承 identity、脱离监督的守护进程不属于受管前台命令支持范围。

## 验证与实际状态

回归入口：

```bash
python3 -m unittest discover -s tests/personal -p test_wsl_compat.py -v
node --test tests/shared-workspace-form.test.mjs tests/skill-write-roots-form.test.mjs
```

编译器可通过 `RUSTC` 指向现有独占工具链；测试编译并运行真实 `personal-runtime/src/wsl.rs`，不复制一份 Python 模型。Windows候选工作流既有的workspace_mapping_tests筛选会执行共享Rust路径/argv回归及Windows参数解析回归；Python整组回归在本地执行。真实WSL探针需显式启用：

```bash
WSL_COMPAT_REAL_DISTRO=Ubuntu-24.04 \
WSL_COMPAT_REAL_ROOT=/home/lixiang/my-project-java/codeVerifyRe0 \
python3 -m unittest discover -s tests/personal -p test_wsl_compat.py -v
```

本次已实际验证 Windows→Ubuntu-24.04 的仓库 cwd、共享规则可读、NVM Node 发现，以及结束自建 Windows relay 后按 identity 回收 Linux 测试子进程。该探针只调用 OS WSL runner，不启动或联系 GPTBridge，不触发 ChatGPT、生产或30项优化。

完整 Windows Tauri 编译、安装替换、当前运行二进制身份和 ChatGPT 经插件执行必须分别留回执；本地共享模块/fixture通过不能替代这四层。
