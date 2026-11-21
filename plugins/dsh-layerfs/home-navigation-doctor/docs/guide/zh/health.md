<!-- source: 0c2bca16c6f8 -->
# 诊断

> **位置：** `LayerFS › 诊断` · **打开方式：** Center [导航栏](components.md#nav-bar)上的诊断按钮（盾牌图标）；机器、Store 或 Sandbox 页面上的 **诊断**（Diagnose） · **源码：** 见文末

当前入口下方的详情图来自早期演示环境。

![1 诊断、2 日志和 3 设置（隔离测试数据）](../img/home-navigation.png)

*隔离测试数据。*

诊断页把整个 LayerFS 主目录画成一棵树（这次安装、它的机器、Store、项目、Sandbox 和备份存储桶），
并显示 LayerFS 的诊断最近一次对每一项说了什么。觉得哪里不对劲时就来这里：检查一下，
再运行 LayerFS 为它给出的那一条修复。打开页面本身不会运行任何诊断：在你按下 **检查**（Check）、
**完整检查**（Full check）或 **全部检查**（Check all）之前，每一行都显示 **未检查**（Not checked）。

![诊断页（带编号）](../img/health-overview.png)

| # | 组件 | 说明 |
|---|---|---|
| 1 | 标题行 | 主目录里最糟的状况：**一切正常**（All clear）、**没有问题**（No problems）、***n* 条提示**（*n* notices）、***n* 个问题**（*n* problems）、**未检查**（Not checked）、**正在检查 *i*/*n***（Checking *i* of *n*），下方是计数（**11 项正常 · 1 条提示**，11 OK · 1 notice） |
| 2 | **全部检查**（Check all） | 把每个诊断各运行一次，并行进行 |
| 3 | 本安装 | **LayerFS *version***：主目录这一行（安装、主目录文件夹、可用磁盘） |
| 4 | 种类标签页 | **机器**（Machines） · **Store**（Stores） · **项目**（Projects） · **Sandbox**（Sandboxes） · **备份**（Backups），每个都带计数，以及一个表示其中最糟一行的圆点 |
| 5 | 一行 | 名称、状态词、相关信息；展开后是它上面的东西 |
| 6 | 检查标记 | 上次诊断做过的每项检查各一个圆点：绿色通过，琥珀色提示，红色失败，灰色未检查 |
| 7 | **检查**（Check） | 运行这一行的诊断 |
| 8 | 来源行 | **从记录读取：`layerfs home get --with tree`。打开此页面不会运行任何医生。**（Read from records: `layerfs home get --with tree`. Opening this page runs no doctor.） |

页面上方是[导航栏](components.md#nav-bar)：`主页 › 诊断`。它的刷新按钮会重新读取这棵树。

## 布局

从上到下依次是：标题行和 **全部检查**（Check all）、主目录这一行、种类标签页、当前标签页的各行、来源行。
点一行的箭头（或点它的名字）就能展开，看到它的详情和直接挂在它上面的东西，所以一个
Store 既能在它所在的机器下面找到，也能在 Store 标签页里找到。之前的检查结果由主机保存，
所以页面打开或重新连上时会直接显示它们。

## 主体

每一行回答的是“它健康吗？不健康的话，怎么修？”。这棵树只来自记录
（`home get --with tree --detail`，读取，2 秒内所有读者共用一次结果）；每次检查是一次
LayerFS 诊断调用，只尝试一次，并随着进度逐项报告。

| 组件 | 显示内容 | 操作 → 结果 | LayerFS 调用 | 状态 |
|---|---|---|---|---|
| 树 | 主目录里的机器、Store、项目、Sandbox（含 Workspace）和存储桶 | 展开一行 → 它的详情和它上面的东西 | `GET /api/doctor/tree` → `home get --with tree --detail` (读取) | **正在读取记录…**（Reading the records…）；**无法读取记录**（The records could not be read），带 **重试**（Try again）；**此 LayerFS 还没有医生**（This LayerFS has no Doctor yet，2.0 之前的运行时） |
| 保存的结果 | 每一行最近一次的结果 | — | `GET /api/doctor/results`（不调用 LayerFS：主机内存） | |
| **全部检查**（Check all） | | 按下 → 每个诊断各运行一次：`home doctor`，每个 Store 一次 `store doctor`，每个 Sandbox 一次 `sandbox doctor`（主目录诊断会快速检查每台机器、每个 PostgreSQL Store 和每个存储桶） | `home doctor`、`store doctor <store>` ×n、`sandbox doctor <sandbox>` ×n (读取) | 运行期间显示忙碌；完整检查或修复运行时不可用（提示会说明原因）。不包括：已停止或有故障的机器上的一切，以及项目数据。提示消息 **全部检查已完成**（Check all finished） |
| **检查**（Check） | 它的提示说明会检查哪些内容，以及具体调用 | 按下 → 运行这一行的诊断；上面有东西的行会自动展开，让结果出现在眼前 | 主目录：`home doctor`；机器：`machine doctor <m>`；Store：`store doctor <s>`；项目：`project doctor <s/p>`；Sandbox：`sandbox doctor <b>`（也包括它的 Workspace）；存储桶：`backup doctor <b>` (读取) | **检查中**（Checking）；另一个调用占着它的执行机会时显示 **等待开始**（Waiting to start），带 **停止**（Stop） |
| **完整检查**（Full check） | 只有项目和存储桶有 | 按下 → 深度检查：项目会读取它的分支能到达的所有内容（可能要几分钟）；存储桶会写入一个小对象再删掉 | `project doctor <s/p> --full`、`backup doctor <b> --full` | 运行时显示已用时间和 **停止**（Stop）；**完整检查在 *time* 时被停止。**（Full check stopped at *time*.） |
| **停止**（Stop） | | 按下 → 结束这个诊断的进程；不写入任何东西 | —（由主机结束 CLI 进程） | 这一行保留上一次的结果 |
| 状态词 | **正常**（OK）、**提示**（Notice）、**问题**（Problem）、**检查中**（Checking）、**未检查**（Not checked）、**已停止**（Stopped）、**无法检查**（Could not check） | 悬停 → 检查时间、数据读取 / 写入测试的时间、为什么在等待 | — | **无法检查**：**已超时。**（Timed out.）、**忙：另一个调用正占用它。**（Busy: another call was holding it.）、**回复丢失。**（The reply was lost.） |
| 等待原因 | **正在等待 *name***（waiting on *name*），带链接 | 点击 → 跳到那一行（必要时切换标签页） | — | 已停止的机器下面的行都在等待这台机器 |
| 详情（展开的行） | **调用**（Call，诊断用的命令，带 **复制**，Copy）、各项检查及其 **通过**（passed） / **提示**（notice） / **失败**（failed） / **未检查**（not checked）和 LayerFS 的说明、**修复**（Fix，修复命令行，带 **复制**，Copy） | **复制**（Copy） → 命令进入剪贴板；不运行任何东西 | — | 还没检查过时：列出它会做的快速检查 |
| 修复按钮 | LayerFS 给出的那一条修复，显示在行上：**启动**（Start）、**重启工作进程**（Restart worker）、**删除项目**（Delete project）、**立即备份**（Back up now） | 按下 → 运行修复，然后再检查一次 | 见[修复](#fixes) | 运行时显示忙碌；**被拒绝**（Refused，琥珀色），附 LayerFS 给的原因；提示消息 ***name* 已就绪**（*name* is ready）、***name* 已删除**（*name* was deleted）、***name* 已备份**（*name* was backed up）、***fix* 被拒绝：*name***（*fix* refused: *name*） |

![Store 标签页上的 guide-notes：检查、提示和修复](../img/health-store.png)

展开后（1），一个 Store 会显示它的 **调用**（Call，3）`layerfs store doctor guide-notes`，以及它的六项
检查。这里 `store.backups` 是一条提示：*它在 guide-backups 里的备份落后了*，所以这一行显示
**提示 · 备份落后**（Notice · Backup behind），并给出 **立即备份**（Back up now，2）。按钮的提示列出了它会运行的命令：
先 `layerfs backup create guide-notes guide-backups`，再 `layerfs store doctor
guide-notes`。**修复**（Fix）这一行（4）就是同一条命令，可以复制。

![项目：完整检查和检查](../img/health-projects.png)

### 修复

只有当某份报告或记录恰好为这一项给出了修复命令，而且是下面这四种之一时，页面才会去运行它。
其他修复命令都只是一行可复制的文字。

| 修复 | 何时给出 | 运行 | 然后 | 撤销 |
|---|---|---|---|---|
| **启动**（Start） | 机器已停止（无论是否有意） | `machine start <m>` | `machine doctor <m>`，以及它上面的东西 | `machine stop` |
| **重启工作进程**（Restart worker） | 机器的 worker 版本过旧（`machine.worker.compatibility`） | 先 `machine stop <m>`，再 `machine start <m>` | 这台机器的各项检查 | — |
| **删除项目**（Delete project） | 项目的记录里有它，但它的 Store 没有列出它（`project.listed`） | 确认后运行 `project delete <s/p>` | 它所在 Store 的诊断 | 无 |
| **立即备份**（Back up now） | Store 在某个存储桶里的备份落后了（`store.backups`） | `backup create <store> <bucket>`（作为一个存储任务） | `store doctor <store>` | — |

每一步只尝试一次。被拒绝是确定的结果（什么都没变；再按一次按钮会重试）；而回复丢失时，
操作可能已经成功，所以不会自动再运行一次：检查这一行，看看它现在是什么状态。

## 变体

- **机器**：本机机器显示三项检查（记录、操作、状态）；Docker 或 Lima
  机器通过它的 worker 显示十项。
- **Sandbox**：Workspace 由它所在 Sandbox 的诊断来检查（**从未检查。由其 Sandbox 检查。**，Never checked. Its
  Sandbox checks it.）。
- **存储桶**：**完整检查**（Full check）还会写入并删除一个小测试对象。
- 记录里什么都没有的标签页显示 **记录中没有这一种类的内容。**（Nothing of this kind is on record.）

## 在此打开的对话框

| 对话框 | 打开方式 | 作用 | LayerFS 调用 | 撤销 |
|---|---|---|---|---|
| **删除 *project*？**（Delete *project*?） | 项目行上的 **删除项目**（Delete project） | *记录中有这个项目，但其 Store 并未列出它。这会运行 `layerfs project delete <s/p>`。* **取消**（Cancel） · **删除项目**（Delete project） | `project delete <s/p>`，然后运行该 Store 的诊断 | 无 |

示例数据里没有哪个项目从它的 Store 中丢失，所以这个[确认对话框](components.md#confirm-dialog)
没有截图。

## 状态与信号

- 种类标签页上的圆点：里面有东西出了问题（红色）、有提示（琥珀色）、未检查
  （灰色）或正在检查；它的提示会统计正在检查、已停止和无法检查的数量。
- 机器上的 **内有 1 条提示**（1 notice inside）：它上面某样东西有提示；展开这一行看看。
- **无法检查 *name*：*reason***（Could not check *name*: *reason*）提示消息：诊断没有回应（超时、忙，或者
  回复丢失）。再按一次 **检查**（Check）。
- **等待开始**（Waiting to start）：这个 Store 上的另一个 LayerFS 调用占着执行机会；**停止**（Stop）会结束
  等待。

## 接下来

- [机器](machine.md)、[Store](store.md)、[Sandbox](sandbox.md)、[存储桶](bucket.md)：
  各行对应的页面（它们的 **诊断**（Diagnose）会打开本页）。
- [日志](logs.md)：本页运行过的诊断和修复调用。
- [组件](components.md#fix-buttons)：其他地方的修复按钮。

## 源码

`src/client/doctor/`：`DoctorPage.tsx`、`Doctor.tsx`、`Row.tsx`、`checks.ts`（每种诊断的
检查项）、`tips.tsx`、`model.ts`、`use-doctor.ts`、`words.ts`、`locale.ts`。主机端：
`src/host/routes/doctor.ts`（`/api/doctor/tree`、`/results`、`/runs`、`/runs/:id/stop`、
`/fixes`）、`src/shared/doctor.ts`（`doctorArgv`、`parseFix`）。
