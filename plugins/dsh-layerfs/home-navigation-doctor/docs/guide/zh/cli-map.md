<!-- source: 45ea773d5f5e -->
# 每个操作及其 LayerFS 调用

每一行是你能做的一件事（或者一个视图为了绘制自己所做的一次读取），它调用的主机路由或操作，以及背后的
LayerFS CLI 调用。各页面指南的表格里也写着同样的调用；这里把它们全部列在一处。

- **路由**（Routes）是 dsh-layerfs 的主机路由（`GET /api/…`），通过 `http` 操作提供。**op** 是有名字的主机操作
  （`session.place`、`sessions.choose`、…）。
- **LayerFS 调用**即 `layerfs <family> <call>`。“(读取)”标记视图为了绘制自己所做的读取。↻ 表示之后会重新读取
  存储视图（`home get`、`project list`、`store auths`、`store list`）。`—` 表示没有 LayerFS 调用：结果来自主机内存、
  主机自己的日志，或当前这个浏览器。
- 一格里的多个调用按顺序运行。第一个被拒绝时，后面的都不会运行。
- 想看某次点击实际运行了哪些调用，打开[日志](logs.md)：那里每一行是一次调用，并标注了发起它的会话。
- 由主机代码生成的完整路由表见 [cli-routes.md](../cli-routes.md)。

## 主页、导航栏和入口

页面：[主页](home.md) · [入口](entry-points.md)

| 操作 | 位置 | 主机路由或 op | LayerFS 调用 |
|---|---|---|---|
| 主页绘制 | `LayerFS › 主页` | `GET /api/storage`, `GET /api/projects` | `home get`, `project list`, `store auths`, `store list` (读取) |
| 诊断计数、主目录检查 | Center 导航栏 › 诊断（计数）、主页 › 顶部卡片 › 刷新（重新检查） | `GET /api/doctor/results`, `POST /api/doctor/runs` `{kind: home}` | 主机内存；`home doctor` |
| 头部卡片 **刷新**（Refresh） | 主页 › 头部卡片 | 存储视图、项目和一次诊断运行 | 上面那些读取，然后 `home doctor` |
| 运行时、设置 | 主页 › 顶部卡片 › 运行时、Center 导航栏 › 设置 | op `setup.status`, `runtime.status` | `home get`（缺失时再 `home init`）；`--version --json` |
| 跳转到、面包屑切换、后退、前进 | 导航栏 | — | — |
| **添加 › 新建 Store**（Add › New Store） | 导航栏 | `POST /api/stores` | `store create` |
| **添加 › 连接数据库**（Add › Connect a database） | 导航栏 | `POST /api/stores/postgresql` | `store save-auth`, `store connect`, `store get` |
| **添加 › 新建机器**（Add › New machine） | 导航栏 | `POST /api/machines` | `home get`, `machine create` |
| **添加 › 添加存储桶**（Add › Add bucket） | 导航栏 | `POST /api/buckets` | `backup connect` |
| 项目行（提交行、计数、大小） | `主页 › 项目` | `GET /api/stores/:id/projects`, `GET /api/projects/:key/size` | `branch get`, `project list`, `project stats`；`project get`, `store get` (读取) |
| 一行下的会话 | `主页 › 项目` | op `sessions.list` | — |
| 打开一条会话 | `主页 › 项目`；侧边栏的 LayerFS 视图 | op `sessions.open` | 在本地则没有；否则 `branch list`, `project export` |
| **⋯ › 新建会话**（⋯ › New session）/ **+ 新建会话**（+ New session） | `主页 › 项目` | op `sessions.choose` | `project get`（首次使用时再加 `branch list`, `branch fork`） |
| **⋯ › 删除项目…**（⋯ › Delete project…） | `主页 › 项目` | `DELETE /api/projects/:key` | `project delete` |
| PostgreSQL 大小和登录 | `主页 › Store › PostgreSQL` | `GET /api/stores/:id/stats`, `GET /api/store-auths` | `store get`；`store auths` |
| 机器表 | `主页 › 机器` | `GET /api/machines`, `GET /api/metrics`, `GET /api/machines/:id/space` | `home get`, `machine get`, `machine list`, `sandbox list`, `store list`；采样器内存；`machine stats` |
| 行内电源 **启动**（Start）、**停止**（Stop）→ **确认：停止**（Confirm: Stop） | `主页 › 机器` | `POST /api/machines/:id/stop` / `start` | `machine stop` / `machine start` |
| 行内 **添加 Store**（Add Store） | `主页 › 机器` | `POST /api/machines/:id/stores` | `store move`（一个任务） |
| 行内 **新建 Sandbox**（New Sandbox） | `主页 › 机器` | `POST /api/projects/:id/sandboxes` | (`machine start`), `project get`, `sandbox create`, `store get` |
| **⋯ › 诊断此主机**（⋯ › Diagnose this host） | `主页 › 机器` | `POST /api/doctor/runs` `{kind: home}` | `home doctor` |
| LayerFS 侧边栏行 | DSH 侧边栏 | — | — |
| Workspace 切换 | DSH 侧边栏 | — （当前浏览器） | — |
| LayerFS 视图的行 | DSH 侧边栏，LayerFS 视图 | op `projects.recent`, `projects.catalog`, `GET /api/storage`, `GET /api/machines` | `project list`, `branch list`, `store get` (读取) |
| 侧边栏中的项目会话 | DSH 侧边栏，LayerFS 视图 | op `sessions.list` | 本地列表每 15 秒一次；归档每 60 秒一次：`branch list`, `file read` |
| **+ 在 {project} 中新建会话**（+ New session in {project}） | DSH 侧边栏，LayerFS 视图 | op `sessions.prepareScratch`, `sessions.choose` | `project get`（再加 `branch list`, `branch fork`） |
| **启动 {machine}**（Start {machine}） | DSH 侧边栏，LayerFS 视图 | `POST /api/machines/:id/start` | `machine start` |
| **归档 {session}**（Archive {session}） | DSH 侧边栏，LayerFS 视图 | op `sessions.archive` | — （DSH 自己的归档） |
| 项目控件：选择项目 | 新建会话行 | op `sessions.choose` | `project get`（再加 `branch list`, `branch fork`），然后 Store 在后台同步 |
| 项目控件标签 | 新建会话行 | op `session.place` | — |
| 草稿目录选择器 | 新建会话行、项目会话 | op `sessions.scratchOptions`, `sessions.scratch` | — |
| 项目对话框：**创建并打开**（Create and open） | 项目对话框 | `POST /api/project-sources/check`, `POST /api/stores`, `POST /api/projects`, `POST /api/machines/:id/stores`, op `sessions.choose` | `store create`, `project init`, `store move`, `project get` |
| 项目对话框：PostgreSQL **连接**（Connect） | 项目对话框 | `POST /api/stores/postgresql` | `store save-auth`, `store connect`, `store get` |
| 项目对话框的读取 | 项目对话框 | `GET /api/storage`, `GET /api/platforms`, `GET /api/store-auths` | 存储视图；`home get`；`store auths` |
| 项目设置：**移到 {machine}**（Move to {machine}） | 项目对话框 › 设置 | `POST /api/machines/:id/stores` | `store move` |
| 项目设置：**从侧边栏隐藏**（Hide from sidebar） | 项目对话框 › 设置 | — （当前浏览器） | — |
| 设置 LayerFS：**检查**（Check）/ **重新检查**（Check again） | 设置 LayerFS | op `setup.status`, `GET /api/machines?fresh=1` | `home get`；`machine get`, `machine list` (读取) |
| 运行时：**使用**（Use）/ **其他构建…**（Other build…） | 设置 LayerFS › **更改…**（Change…） | op `runtime.save` | `--version --json`, `help --json`, `machine update native` |
| **使用此守护进程**（Use this daemon） | 设置 LayerFS | op `setup.recordDaemon` | `machine get native`, `machine update native --daemon` |

## Store 和存储桶

页面：[Store](store.md) · [存储桶](bucket.md)

| 操作 | 位置 | 主机路由或 op | LayerFS 调用 |
|---|---|---|---|
| 打开 Store 页面 | `LayerFS › 主页 › Store › <store>` | `GET /api/storage` | `home get`, `project list`, `store auths`, `store list` (读取) |
| 概览的文件卡片和磁贴 | `… › <store> › 概览` | `GET /api/stores/:id/stats` | `store get --with stats` (读取) |
| Store 的增长（How the Store grew） | `… › <store> › 概览` | `GET /api/stores/:id/observations` | — （主机记录的历次读取） |
| 项目磁贴、项目标签页、卡片行 | `… › <store> › 项目` | `GET /api/stores/:id/projects` | `project list`, `project stats`, `branch get` (读取) |
| 正在运行磁贴、项目的 Sandbox | `… › <store>` | `GET /api/machines` | `home get`, `machine list`, `machine get`, `sandbox list`, `store list` (读取) |
| 项目会话列 | `… › <store> › 项目` | op `sessions.list` | — （当前这个 DSH） |
| **检查完整性**（Check integrity）/ **测试连接**（Test connection） | `… › <store>` 头部 | `POST /api/doctor/runs` `{kind: store}` | `store doctor` |
| **⋯ › 推送到…**（⋯ › Push to…）→ **推送**（Push） | `… › <store> ⋯` | `POST /api/stores/:id/push` | `store move <s> <m>`（一个任务）, `backup list` |
| **⋯ › 拉回 Home**（⋯ › Pull to home）/ 卡片 **拉取**（Pull） | `… › <store> ⋯` | `POST /api/stores/:id/pull` | `store get`, `store move <s> native` |
| **⋯ › 迁移到其他机器…**（⋯ › Move to another machine…）→ **迁移**（Move） | `… › <store> ⋯` | `POST /api/stores/:id/move` | `store get`，经 native 两次 `store move` |
| **⋯ › 备份到…**（⋯ › Back up to…）→ **备份**（Back up）/ **立即备份**（Back up now） | `… › <store> ⋯`、备份标签页 | `POST /api/stores/:id/backup` | `backup create`，然后 `backup list` |
| **⋯ › 压缩…**（⋯ › Compact…）→ **压缩**（Compact） | `… › <store> ⋯` | `POST /api/stores/:id/compact` | `store get`, `store compact` |
| **⋯ › 移除…**（⋯ › Remove…）→ **移除**（Remove） | `… › <store> ⋯`、Store 详情 | `DELETE /api/stores/:id?confirm=` | `store remove` ↻ |
| **⋯ › 断开连接…**（⋯ › Disconnect…）（PostgreSQL） | `… › <store> ⋯` | `POST /api/stores/:id/disconnect` | `store disconnect` ↻ |
| **⋯ › 断开 <machine>**（⋯ › Disconnect <machine>）/ 连接 › **忘记**（Connection › Forget） | `… › <store>` | `POST /api/stores/:id/connection` `{machine, clear}` | `store disconnect --machine` ↻ |
| **⋯ › 检查**（⋯ › Check）、连接 › **检查**（Connection › Check）、一个“连接自”标签、**连接一台机器**（Connect a machine） | `… › <store>` | `GET /api/stores/:id/check?machine=` | `store doctor --machine`, `store get` |
| **更换登录…**（Change login…）→ 编辑连接 | `… › <store> › 连接` | `POST /api/stores/:id/connection` | `store connect`, `store save-auth`, `store update`, `store get` ↻ |
| **为 <m> 设置专属地址**（Give <m> its own address）→ **保存并检查**（Save and check） | `… › 连接`、检查界面 | `POST /api/stores/:id/connection` `{machine, url}` | `store connect --machine`，然后 `store doctor --machine` |
| **保存登录…**（Save a login…）→ **保存**（Save） | `… › <store>` | `POST /api/stores/:id/connection` | `store save-auth`, `store update --auth` ↻ |
| **更换密码…**（Rotate password…）→ **保存并检查**（Save and check） | `主页 › Store` 的 PostgreSQL 登录标题 | `POST /api/store-auths/:id/rotate` | `store save-auth`，每个 Store 一次 `store doctor`, `store get` ↻ |
| **移除登录**（Remove login） | `主页 › Store` 的 PostgreSQL 登录 ⋯ | `DELETE /api/store-auths/:id` | `store remove-auth` ↻ |
| 已保存的登录列表 | PostgreSQL 对话框 | `GET /api/store-auths` | `store auths` (读取) |
| **添加项目**（Add project）→ **创建项目**（Create project） | `… › <store> › 项目` | `POST /api/projects`, `GET /api/project-inits/:id` | `project init` |
| 备份标签页 › **恢复…**（Restore…）→ **恢复 Store**（Restore Store） | `… › <store> › 备份` | `POST /api/buckets/:id/restore` | `backup restore`，然后 `backup list` |
| 备份徽标 → <store> 的备份 | Store 卡片 | `GET /api/buckets/:id/files`（每个关联的存储桶） | `backup list` (读取) |
| 备份对话框 › **更新**（Update）/ **立即备份**（Back up now） | 备份对话框 | `POST /api/stores/:id/backup` | `backup create`, `backup list` |
| **添加 › 新建 Store**（Add › New Store）→ **创建 Store**（Create Store） | 导航栏“添加”、主页 › Store、native › Store | `POST /api/stores`（再加 `POST /api/machines/:m/stores`） | `store create` ↻（然后用 `store get`, `store move` 把它放到位） |
| 新建 Store › 从备份恢复 → **恢复 Store**（Restore Store） | 新建 Store 对话框 | `POST /api/buckets/:id/restore`（再加 `POST /api/machines/:m/stores`） | `backup restore`, `backup list`（然后 `store move`） |
| 新建 Store › **列出存储桶**（List the bucket） | 新建 Store 对话框 | `GET /api/buckets/:id/files` | `backup list` (读取) |
| 新建 Store 或推送对话框里的 **启动 <machine>**（Start <machine>） | 对话框 | `POST /api/machines/:id/start` | `machine start` |
| **添加 › 连接数据库**（Add › Connect a database）→ **连接**（Connect） | 导航栏“添加”、主页 › Store › PostgreSQL、某台机器的 Store | `POST /api/stores/postgresql` | `store save-auth`（新登录）, `store connect`, `store get` ↻，然后 `store doctor --machine` |
| 把 Store 推送到这里 / **添加 Store**（Add Store）→ **移动到这里**（Move here） | `机器 › <m> › Store` | `POST /api/machines/:id/stores` | `store get`, `store move`（推送、拉取，或先拉取再推送） |
| 任务 **重试**（Retry） | 任务卡片 | 再次调用该任务的路由 | 该任务自己的调用 |
| 任务 **关闭**（Dismiss） | 任务卡片 | — | — （当前浏览器） |
| 待处理提示 **启动 <machine>**（Start <machine>） | Store 页面提示、卡片状态 | `POST /api/machines/:id/start` | `machine start` |
| **添加 › 添加存储桶**（Add › Add bucket）→ **连接存储桶**（Connect bucket） | 导航栏“添加”、备份对话框、主页 › Store › 存储桶 | `POST /api/buckets` | `backup connect`（密钥经 stdin 传入） ↻ |
| **重新连接…**（Connect again…）→ **重新连接**（Connect again） | 存储桶页面、存储桶卡片 | `POST /api/buckets`, `GET /api/buckets/:id/files` | `backup connect`, `backup list` |
| 打开存储桶页面、**列出**（List）/ **重新列出**（List again）、卡片 **⋯ › 列出文件**（⋯ › List files） | `LayerFS › … › <bucket>` | `GET /api/buckets/:id/files` | `backup list` (读取) |
| **列出存储桶文件**（List bucket files） | `主页 › Store › SQLite › 存储桶` | `POST /api/storage/check` | 每个存储桶一次 `backup list`，然后是存储视图的读取 |
| 存储桶 **测试连接**（Test connection） | `… › <bucket>` | `POST /api/doctor/runs` `{kind: bucket}` | `backup doctor` |
| 存储桶 **⋯ › 断开连接**（⋯ › Disconnect）→ **断开连接**（Disconnect） | `… › <bucket>` | `DELETE /api/buckets/:id` | `backup disconnect` ↻ |
| 存储桶行或文件行的 **立即备份**（Back up now） | `… › <bucket>`, `… › <bucket> › <file>` | `POST /api/stores/:s/backup` `{bucket, file}` | `backup create`, `backup list` |
| **恢复…**（Restore…）/ **恢复为新 Store…**（Restore as a new Store…）→ **恢复 Store**（Restore Store） | 存储桶页面、文件页面、存储桶卡片 | `POST /api/buckets/:id/restore` | `backup restore`, `backup list` |

## 项目

页面：[项目](project.md)

| 操作 | 位置 | 主机路由或 op | LayerFS 调用 |
|---|---|---|---|
| 打开项目页面（文件标签页） | `LayerFS › 主页 › Store › <store> › <project>` | `GET /api/projects/:id/branches` | `branch list`, `branch get main` (读取) |
| 打开文件夹 | `… › <project> › 文件` | `GET /api/projects/:id/fs` | `file list` (读取)；只有还不知道已保存的 id 时才 `branch get` |
| 打开文件（代码）、**显示更多行**（Show more lines） | `… › 文件` | `GET /api/projects/:id/file` | `file stat`, `file read` (读取) |
| **逐行追溯**（Blame） | `… › 文件 › 逐行追溯` | `GET /api/projects/:id/file-query?kind=blame`（再加 `kind=revisions`） | `file read`, `file blame`, `file history`，每个尚未命名的所有者一次 `commit get` (读取) |
| 文件 **历史**（History）、**包含其他分支**（Include other branches）、**加载更早的版本**（Load earlier versions） | `… › 文件 › 历史` | `GET /api/projects/:id/file-query?kind=revisions` | `file history`；包含其他分支时还有 `file history --traversal branch`，以及每个保存在别处的版本一次 `commit get` (读取) |
| 某个版本的差异、**对比**（Compare with:） | `… › 文件 › 历史` | `GET /api/projects/:id/file-query?kind=compare` | `commit diff --path --with patch` (读取)；非文本文件还会对两侧各做一次 `file stat --at` (读取) |
| 分支选择器、**加载更多分支**（Load more branches） | `… › 文件` | `GET /api/projects/:id/branches?pos=` | `branch list` (读取) |
| 打开历史标签页、**刷新**（Refresh） | `… › 历史` | `GET /api/projects/:id/history`（先读分支列表，再读各页） | `branch list`, `branch get main`, `commit list` (读取) |
| **全部加载**（Load all）（后台读取） | `… › 历史` | `GET /api/projects/:id/history?branch=&at=&wide=1` | `commit list` (读取) |
| 旁支分支的总数 | `… › 历史` | `GET /api/projects/:id/history?…&count=1` | `commit list` 宽页 (读取；按分支头缓存) |
| **停止**（Stop）、缩放、**完整历史**（Whole run）、折叠一个分支族 | `… › 历史` | — | — （视图状态） |
| 选择叶节点、Layer 或分支标签 | `… › 历史` | `GET /api/projects/:id/history?branch=&commit=` | `commit get`、只取路径的 `commit diff` (读取) |
| **查看差异**（View diff） | `… › 历史 › <id> 中的更改` | `…/history?commit=` + `GET /api/projects/:id/file-query?kind=compare` | `commit get`, `commit diff`, `commit diff --path --with patch` (读取) |
| **对比**（Compare）/ **与…对比**（Compare with…），然后选一个版本或分支标签 | `… › 历史 › 对比版本` | `GET /api/projects/:id/file-query?kind=branch-diff`，然后 `kind=compare` | `commit diff` (读取), `commit diff --path --with patch` (读取) |
| **在此提交中打开**（Open at this commit）/ 点击一个改动的路径 | 历史 → 文件 | 该版本的 `GET /api/projects/:id/fs`, `/file` | `file list`, `file stat`, `file read` (读取) |
| 分支查找器 › **加载更多分支**（Load more branches） | `… › 历史` | `GET /api/projects/:id/history?pos=` | `branch list` (读取) |
| Sandbox 标签页（列表） | `… › <project> › Sandbox` | `GET /api/projects/:id/sandboxes`, `GET /api/sandboxes/:id` | `sandbox list` (读取), `sandbox get` (读取，主机缓存) |
| Sandbox 标签页（地图上的调用） | `… › Sandbox` | op `logs.project` | — （插件的调用日志） |
| 关闭 Sandbox（**强制关闭**（Force close）） | `… › Sandbox` | `DELETE /api/sandboxes/:id[?force=1]` | `sandbox remove` |
| 会话标签页 | `… › <project> › 会话` | op `sessions.list` | `branch list --prefix dsh.session.` (读取)，已迁移归档的 `project export` (读取) |
| **新建会话**（New session） | `… › 会话` | DSH 新建会话 + op `sessions.choose`（没有选文件夹时再加 `sessions.prepareScratch`） | `project get` (读取)；会话存储在后台同步；Workspace 在第一步时挂载 |
| **打开**（Open）（一个会话） | `… › 会话` | op `sessions.open` | 在本地则没有；导入已存储的会话时 `project export` (读取) |
| 统计标签页、**重新统计**（Recount） | `… › <project> › 统计` | `GET /api/projects/:id/storage[?refresh=1]` | `project get` (读取，缓存), `store get` ×2, `project stats`；账本：`branch list`, `commit list` (读取) |
| **查看 Store 用量**（View Store usage） | `… › 统计` | — （打开 Store 页面） | — |
| **刷新项目**（Refresh project） | 项目标签栏 | 再次调用当前标签页的路由 | 同该标签页 |
| **⋯ › 删除项目…**（⋯ › Delete project…）→ **删除**（Delete） | 项目标签栏 | `DELETE /api/projects/:id` | `project delete` |

## 机器和 Sandbox

页面：[机器](machine.md) · [Sandbox](sandbox.md)

| 操作 | 位置 | 主机路由或 op | LayerFS 调用 |
|---|---|---|---|
| 机器页面、机器看板 | `主页 › 机器`, `… › <machine>` | `GET /api/machines` | `home get`, `machine list`, `machine get --with live`, `sandbox list`, `store list` (读取) |
| **重新检查**（Check again）（修复面板） | 机器提示、添加机器 | `GET /api/machines?fresh=1` | 同样的读取，平台信息重新读取 |
| 用量卡片、看板内存、Sandbox 数值 | 概览、看板、Sandbox 标签页、Sandbox 概览 | `GET /api/metrics` | — （采样器内存；采样器每 15 秒对每台机器运行一次 `machine stats`） |
| 看板磁盘、用量的磁盘拆分、**磁盘占用**（On disk） | 看板、概览、Sandbox 概览 | `GET /api/machines/:id/space` | `machine stats --with space` (读取，缓存 1 分钟) |
| 磁盘空间对话框、**刷新**（Refresh） | `… › <machine> › 概览` | `GET /api/machines/:id/space[?fresh=1]` | `machine stats --with space` (读取) |
| **运行诊断**（Run diagnostics） | 机器概览 | 诊断（`/api/doctor/…`） | `machine doctor` |
| **诊断此主机**（Diagnose this host） | `主页 › 机器` ⋯ | 诊断 | `home doctor` |
| **启动 worker**（Start worker）/ **启动**（Start） | 机器头部、看板、菜单 | `POST /api/machines/:id/start` | `machine start` |
| **停止 worker**（Stop worker）/ **停止**（Stop） | 机器头部、看板、菜单 | `POST /api/machines/:id/stop` | `machine stop` |
| **重启 worker**（Restart worker）（修复） | 机器头部、菜单 | `POST /api/machines/:id/restart` | `machine get`, `machine stop`, `machine start` |
| **重启 worker**（Restart worker）（worker 已过时） | `… › <machine> › 镜像` | 先 `POST …/stop` 再 `POST …/start` | `machine stop`, `machine start` |
| **强制停止…**（Force stop…）→ **强制停止**（Force stop） | 机器 ⋯ | `POST /api/machines/:id/stop` `{force}` | `machine stop --force` |
| **更新设置…**（Update settings…）→ **保存**（Save） | 机器 ⋯、详情 | `POST /api/machines/:id/update` | `machine update` |
| **移除…**（Remove…）→ **移除**（Remove）/ **全部移除**（Remove everything） | 机器 ⋯、危险操作 | `DELETE /api/sandboxes/:s?force=1` ×n, `DELETE /api/stores/:s` ×n, `DELETE /api/machines/:id` | `sandbox remove --force`, `store remove`, `machine remove` |
| **新建机器**（New machine）→ **创建**（Create） | `主页 › 机器`、添加菜单 | `POST /api/machines` | `home get`, `machine create` |
| **检查当前状态**（Inspect current state） | 添加机器，失败之后 | `GET /api/machines/:id/inspect` | `machine get` (读取) |
| 新建 Sandbox：项目列表 | 新建 Sandbox 对话框 | `GET /api/projects` | `project list` (读取) |
| 新建 Sandbox：镜像菜单、镜像标签页 | 新建 Sandbox 对话框、`… › 镜像` | `GET /api/machines/:id/sandbox-images` | `machine get`, `machine images --with docker`, `sandbox list` (读取) |
| 新建 Sandbox → **创建**（Create）/ **启动并创建**（Start and create） | 新建 Sandbox 对话框 | `POST /api/projects/:id/sandboxes` `{background}` | `store get`, `project get`, (`machine start`), `sandbox create` |
| **重试**（Retry）一次失败的创建 | 创建中的行 | `POST /api/projects/:id/sandboxes` | `sandbox create` |
| **关闭**（Dismiss）一次失败的创建 | 创建中的行 | `DELETE /api/sandbox-creates/:id` | — （仅主机） |
| 把镜像复制到机器 | `… › 镜像` 的 Docker 列表 | `POST /api/machines/:id/load-image` | `machine load-image`（在后台） |
| **关闭**（Dismiss）一次失败的复制 | `… › 镜像` | `DELETE /api/image-loads/:id` | — （仅主机） |
| 移除一个镜像 / **释放空间…**（Free space…）→ **移除 n 个镜像**（Remove n images） | 镜像卡片、释放空间对话框 | `POST /api/machines/:id/prune-images` | 每个镜像一次 `machine prune <m> <image>` |
| 文件标签页（机器） | `… › <machine> › 文件` | `GET /api/machines/:id/files` | `machine files` (读取) |
| 取消文件或日志读取、离开标签页 | 文件、日志 | op `inspect.cancel` | — （仅主机） |
| Worker 日志 | `… › <machine> › Worker 日志` | `GET /api/machines/:id/log` | `machine log` (读取) |
| Sandbox 页面、**刷新**（Refresh） | `… › <sandbox>` | `GET /api/sandboxes/:id` | `sandbox get` (读取) |
| **诊断**（Diagnose） | Sandbox 概览 | 诊断 | `sandbox doctor` |
| **删除 Sandbox…**（Delete Sandbox…）→ **删除 Sandbox**（Delete Sandbox） | Sandbox ⋯、卡片 ⋯、看板行 ⋯ | `DELETE /api/sandboxes/:id` | `sandbox remove`（对话的 Sandbox：`workspace status`, `workspace commit`, `sandbox remove --force`） |
| **强制删除 Sandbox…**（Force delete Sandbox…） | 同上 | `DELETE /api/sandboxes/:id?force=1` | `sandbox remove --force` |
| **关闭**（Close）（已消失的 Sandbox） | Sandbox 卡片 | `DELETE /api/sandboxes/:id` | `sandbox remove` |
| 新建 Workspace：分支列表 | `… › <sandbox> › Workspace` 表单 | `GET /api/projects/:id/branches` | `branch list`, `branch get` (读取) |
| **挂载**（Mount） | Workspace 表单 | `POST /api/sandboxes/:id/workspaces` | `workspace mount`（新分支：先 `sandbox get`, `branch fork`） |
| 运行命令（Enter） | 终端窗格 | `POST /api/workspaces/:ws/commands` | `workspace exec` |
| Ctrl+C | 终端窗格 | `POST /api/workspaces/:ws/cancel` | `workspace cancel --exec` |
| 命令记录 | 终端窗格 | `GET /api/workspaces/:ws/commands` | — （仅主机） |
| **提交…**（Commit…）→ **提交**（Commit） | 终端 ⋯ | `POST /api/workspaces/:ws/commit` | `workspace commit` |
| × 关闭窗格 | 终端头部 | `DELETE /api/workspaces/:ws?discard=1&force=1` | `workspace unmount --discard --force` |
| Sandbox 文件（Sandbox 文件夹、整个文件系统） | `… › <sandbox> › 文件` | `GET /api/sandboxes/:id/files` | `sandbox files` (读取) |
| Sandbox 文件（Workspace 视图） | `… › <sandbox> › 文件` | `GET /api/projects/:id/fs`, `/file` | `branch get`, `file list`, `file read`, `file stat` (读取) |
| Daemon 日志 | `… › <sandbox> › Daemon 日志` | `GET /api/sandboxes/:id/log` | `sandbox log` (读取) |

## 项目会话、诊断、日志和预设

页面：[对话](conversation.md) · [诊断](health.md) · [日志](logs.md) · [预设](presets.md)

| 操作 | 位置 | 主机路由或 op | LayerFS 调用 |
|---|---|---|---|
| LayerFS 标签页出现；项目会话标记 | 任意对话的标题栏 | op `session.place` | — （主机内存） |
| 轨迹标签页绘制 | 项目会话 › 轨迹 | op `session.trajectory` | — （会话日志及其 LayerFS 注解） |
| 轮次和 LayerFS 调用的折叠、**只看 LayerFS 写入**（Only LayerFS writes）、搜索、**显示更多行**（Show more rows）、点击时间线、上下文消息 | 轨迹工具栏和表格 | — | — （视图状态） |
| 检查器的 Payload / Result | 轨迹 › 一条记录 › 检查器 | op `session.trajectory.record` | — （会话日志） |
| 版本标签、**在历史中打开**（Open in History） | 轨迹行或检查器 | 打开历史标签页 | 历史视图的读取（`branch list`, `commit list`, `commit diff`） |
| 会话行、未保存计数 | 文件、历史、统计标签页 | op `session.state` | `workspace status --detail`, `project get`, `branch list --prefix dsh.project.`, `machine get`, `store get` (读取) |
| 文件标签页 | 项目会话 › 文件 | `GET /api/projects/:id/fs`, `/file`, … | `file list`, `file read`, `file stat`, `branch get` (读取) |
| 统计标签页 | 项目会话 › 统计 | `GET /api/projects/:id/storage` | `project get`, `project stats`, `store get` (读取) |
| Sandbox 地图上的 agent、活动 | 项目会话 › Sandbox | op `logs.list` `{tag: dsh:<session>:}` | `home log --tag` (读取，每页 1000 条) |
| Sandbox 地图上的机器标记 | 项目会话 › Sandbox | `GET /api/machines` | `home get`, `machine list`, `machine get`, `sandbox list`, `store list` (读取) |
| 播放、暂停、重播、速度、**实时**（Live）、拖动进度、此对话 | 项目会话 › Sandbox | — | — |
| 活动的调用详情 | 项目会话 › Sandbox | op `logs.output`, `session.trajectory.record` | — （保留的输出） |
| **查看脚本**（View script）及其对话框 | 脚本会话的标题栏 | op `simulation.script`, `simulation.status` | — （保存的脚本文件） |
| 诊断页面绘制 | `LayerFS › 诊断` | `GET /api/doctor/tree`, `GET /api/doctor/results` | `home get --with tree --detail` (读取)；结果：— |
| **检查**（Check） | 诊断中的一行 | `POST /api/doctor/runs` | `home doctor` / `machine doctor <m>` / `store doctor <s>` / `project doctor <s/p>` / `sandbox doctor <b>` / `backup doctor <b>` |
| **完整检查**（Full check） | 诊断中的项目行或存储桶行 | `POST /api/doctor/runs` `{full}` | `project doctor --full`, `backup doctor --full` |
| **全部检查**（Check all） | 诊断头部 | `POST /api/doctor/runs`（一组） | `home doctor`, `store doctor` ×n, `sandbox doctor` ×n |
| **停止**（Stop） | 诊断中的一行 | `POST /api/doctor/runs/:id/stop` | — （停止正在运行的 CLI 进程） |
| **启动**（Start）/ **重启工作进程**（Restart worker）/ **立即备份**（Back up now） | 诊断行的修复 | `POST /api/doctor/fixes` | `machine start` / `machine stop` + `machine start` / `backup create <s> <b>`；然后再次运行该对象的诊断 |
| **删除项目**（Delete project）→ 确认 | 诊断中的项目行 | `POST /api/doctor/fixes` | `project delete <s/p>`，然后是该 Store 的诊断 |
| **复制**（Copy）（调用、修复） | 诊断行详情 | — | — |
| 日志表格、筛选、**刷新**（Refresh）、**加载更早的调用**（Load older calls）、标签 chip、**显示全部调用**（Show all calls） | `LayerFS › 日志` | op `logs.list` | `home log --detail` 加 `--call` / `--client` / `--tag` / `--outcome` / `--cursor` (读取)；CLI 没有 `home log` 时读替代文件 |
| **重置…**（Reset…）→ **重置**（Reset） | 日志头部 | op `logs.reset` | `home clear-log`（或替代文件），以及保留的输出 |
| 调用详情输出、**重试**（Retry） | 日志 › 一行 | op `logs.output` | — （重新读取保留的输出；不会再次运行该调用） |
| **复制**（Copy）、**打开会话**（Open session）、上一个 / 下一个 | 日志 › 一行 | — | — |
| **此会话的调用 / 此检查点的调用**（Calls of this session / checkpoint） | 日志 › 一行 | op `logs.list` `{tag}` | `home log --tag` (读取) |
| 预设对话框、预设列表、状态、资源详情 | `主页 ›` 试用预设 | `GET /api/demo-presets` | — （主机记录） |
| **查看脚本**（View script）、项目的 **#** | 预设 | `GET /api/demo-presets/:id/script` | `machine list`, `store list`, `sandbox list`, `sandbox get` ×n (读取) |
| **构建…**（Build…）/ **重新构建…**（Rebuild…）→ **继续**（Continue） | 预设 | `GET /api/demo-presets/:id/build-plan` | `home get --with platforms`, `machine list`, `store list`, `sandbox list`, `sandbox get` (读取) |
| **清空并构建**（Wipe and build） | 预设计划审阅 | `POST /api/demo-presets/:id/environment-reset` `{rebuild: true}` | 先重置（`machine start`, `workspace unmount --discard`, `sandbox remove`, `store remove`, `store disconnect`, `machine remove`, `machine stop docker`），再执行配方（`store create`, `project init`, `file write`, `branch fork` / `merge` / `publish`, …；混合模式还会加上 `workspace mount` / `exec` / `commit`），最后用读取做验证 |
| **停止机器…**（Stop machines…）（打开） | 预设 ⋯ | `GET /api/demo-presets/:id/environment-plan` | `machine list` (读取) |
| **停止机器**（Stop machines） | 预设确认 | `POST /api/demo-presets/:id/environment-reset` `{rebuild: false}` | 对每台机器运行 `machine stop <m> --force`；项目会话会被删除 |
| **检查操作**（Check operation） | 预设失败提示 | `GET /api/demo-presets/requests/:id` | — |
