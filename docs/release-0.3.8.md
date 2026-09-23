# 0.3.8：可配置高并发与控制容量保留

基于0.3.7提交a18b87516aa1dd1dfbe4a6f28e4f1342249857e2，保留其可信虚拟环境、lsof和显式服务重启修复。

新增performance档：命令64、其中重构建4、作业排队加执行256；HTTP在途256、长等待192、控制预留32；stdio线程32（控制4）、队列256。当前进程句柄预算请求4096，受硬限额约束，不修改系统全局值。

共享配置在工作区持久状态目录concurrency.json；server_info返回实际路径、有效值和重启要求。无配置保持保守默认。配置漂移拒绝新工作，但保留旧回执；源锁、输出资源锁、权限和SQLite FULL不变。slot锁算法没有修改。

16项新增测试包含96个真实worker/64同时运行、4构建许可、256活跃队列、192HTTP同时等待、stdio300请求的控制保留和过载错误、配置校验及受控LaunchAgent身份核验。新建192个TCP连接的瞬时洪峰受本机128系统待接入队列限制；测试分批建连后仍保持192同时在途，没有调整系统网络参数。

最终0.3.8完整14阶段回归通过：/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal/.artifacts/selftest/1790145494222520000/result.json。
源码指纹：a2fea2d2e9e02ec1a795c9751f999ca5bfc6923cb722de7b82b107a345be8984。
应用打包通过：/Users/didi/my-project-java/codeVerifyRe0/coding-tools-mcp-personal/.artifacts/checks/bundle-038-1790145835598487000/result.json。

详细配置、压测和边界见docs/high-concurrency.md。安装、实际服务版本和performance启用结果以.artifacts/concurrency037运行回执为准，不用构建成功替代线上验证。
