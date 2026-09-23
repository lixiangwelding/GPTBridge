# GPTBridge 0.4.1

主名：**GPTBridge**。中文定位：**AI 任务工作台**。

主文案：**把对话，接到你的工作现场。**

中文短描述：连接 ChatGPT 与本地项目、工具和技能，集中管理任务进展、执行记录与接续上下文。

English: Connect ChatGPT to local projects, tools and skills. Keep task progress, execution records and resumable context in one place.

## 变更边界

本轮更新应用窗口、托盘、导航品牌、页面标题、提示文案、软件包元信息、README 和使用说明。版本统一为 0.4.1；不将源码版本当作当前运行版本。

保留 `com.lixiangwelding.codingtools.personal`、`coding-tools-mcp-personal` 二进制名、`taskdock_*` IPC、`taskdock:` 存储键、现有数据路径与命令行标志。它们属于兼容合同，不是漏改的展示名称。旧 release 和设计规格也保留原名，避免篡改历史证据。既有安装路径由原运维脚本继续定位，不在没有迁移验收时自动切换到一个新包。

## GitHub 与截图

应用改名、GitHub 仓库改名、GitHub About 描述更新、源码推送、安装替换是独立步骤。没有远端成功回执时不更改 origin 或编造新仓库 URL；当前应用中的仓库入口指向已有的个人仓库，而非上游安装包。

截图需从实际编译的前端重新采集，保存环境、路由、视口、源码与图片 SHA。浏览器预览没有 Tauri IPC 时，应保留“原生服务未连接”提示，不注入假任务，也不将截图标为原生任务执行验收。旧原型和旧文档图片保持不变，不给旧图只改文字后冒充重拍。

## 来源说明

独立项目，不是 OpenAI 官方产品或官方背书。保留上游 Apache-2.0 许可证、NOTICE 和代码来源。名称创意不代表商标检索或公开发行许可已经完成。
