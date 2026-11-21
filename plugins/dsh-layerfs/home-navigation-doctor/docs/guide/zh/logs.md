<!-- source: 41b39b388cbd -->
# 日志

> **位置：** `LayerFS › 日志` · **打开方式：** Center [导航栏](components.md#nav-bar)上的日志按钮（列表图标）；在调用详情里点 **此会话的调用**（Calls of this session）/ **此检查点的调用**（Calls of this checkpoint）会带着筛选重新打开它 · **源码：** 见文末

当前入口下方的详情图来自早期演示环境。

![1 诊断、2 日志和 3 设置（隔离测试数据）](../img/home-navigation.png)

*隔离测试数据。*

日志页按时间倒序列出这个 LayerFS 主目录上的每一次 LayerFS 调用：一行一个调用，写明确切的
CLI 命令、结果、保存了什么；如果是 DSH 发起的调用，还会写明它属于哪个会话、哪一轮、哪个检查点。
想知道某次点击到底跑了哪条 CLI 调用，就来这里看：在应用里随便按一个按钮，再看这一页的最上面。

![日志页，带编号标注](../img/logs-overview.png)

| # | 组件 | 说明 |
|---|---|---|
| 1 | **刷新**（Refresh） | 从头重新读取日志 |
| 2 | **重置…**（Reset…） | 清空调用历史（会先确认） |
| 3 | 项目搜索 | 只看提到某个项目（`store/project`）的调用 |
| 4 | **操作**（Operation） | 只看某一类或某一个调用（`workspace`、`workspace.commit`…） |
| 5 | **客户端**（Client） | **全部**（All）· **cli** · **node** · **python** · **rust** |
| 6 | **结果**（Outcome） | **全部**（All）· **成功**（OK）· **被拒绝**（Refused） |
| 7 | **仅写入**（Writes only） | 隐藏读取（默认打开） |
| 8 | 命令 | 调用原样的写法：`layerfs workspace commit dsh-…:main --message '<10 bytes>' --detail` |
| 9 | 已保存 | 调用保存了什么：**已提交**（committed）/ **已分叉**（forked）/ **已合并**（merged）/ **已发布**（published）/ **无变化**（unchanged），以及一个短 id |
| 10 | 会话 | 它所属的会话和位置：**第 3 轮 · 轮次检查点**（turn 3 · turn checkpoint）、**第 1 轮 · 工具调用**（turn 1 · tool call）、**主机保存**（host save）、**第 0 轮 · 起点 · 引用**（turn 0 · base · references） |

页面上方是[导航栏](components.md#nav-bar)：`主页 › 日志`。

## 布局

先是标题行（**日志**（Logs）、**刷新**（Refresh）、**重置…**（Reset…）），然后是筛选条件，再下面是调用表格，
表格底部有 **加载更早的调用**（Load older calls）。点一行会打开这次调用的详情。

## 主体

这张表回答的是“LayerFS 做了什么、为谁做的、成功了没有”。页面读取的是 LayerFS 自己为这个主目录记的
调用日志（`home log`），凡是使用这个主目录的客户端都会往里写（DSH、LayerFS Connect、终端、脚本）。
如果运行时还没有 `home log`，页面就读插件的替代日志（`trace/calls.jsonl`），里面只有 DSH 发起的调用。
一行就是一次调用；DSH 给自己的每次调用打上 `dsh:<session>:<receipt>` 标签，主机据此在这一行标出
会话标题、轮次和检查点。

| 组件 | 显示内容 | 操作 → 结果 | LayerFS 调用 | 状态 |
|---|---|---|---|---|
| 表格 | 每页 100 次调用，最新的在前：**时间**（Time，相对时间，悬停显示确切时间）、**客户端**（Client）、**命令**（Command，类别和调用名加粗；内容显示为 `'<N bytes>'`）、**结果**（Outcome）、**耗时**（Duration）、**已保存**（Saved）、**会话**（Session） | 点击一行或按 Enter → 打开调用详情。↑/↓ 在行之间移动。悬停或聚焦命令 → 显示完整命令 | `logs.list` → `home log --detail` (读取)，筛选条件以 `--call`、`--client`、`--tag`、`--outcome` 传入 | **正在读取调用…**（Loading calls…）；**无法读取调用日志**（Could not read the call log），附 **重试**（Retry） |
| **刷新**（Refresh） | | 按下 → 重新读取第一页 | `home log` (读取) | 读取期间不可用。每来一次新调用（主机的 `logs` 事件）、重新连接、窗口获得焦点时，页面也会重新读取，已加载的页保留 |
| 项目搜索 | | 输入 `store/project` → 只看提到它的调用（稍等片刻生效）；按 Escape 清空 | `home log` (读取；在这里按子串匹配) | |
| **操作**（Operation） | 目前见过的类别和调用 | 选择 → 只看这些 | `home log --call` (读取) | |
| **客户端**（Client） | | 选择 → 只看这个客户端 | `home log --client` (读取) | |
| **结果**（Outcome） | | **成功**（OK）→ 只看成功的；**被拒绝**（Refused）→ 其余所有结果 | `home log --outcome ok` (读取)；**被拒绝**（Refused）在这里筛选 | |
| **仅写入**（Writes only） | 默认打开 | 关掉 → 读取也显示 | —（由主机筛选） | **还没有写入**（No writes yet）· *读取调用已隐藏。*（Read calls are hidden.）**显示读取**（Show reads） |
| 结果单元格 | **成功**（OK），或红色的 LayerFS 错误码（`NotFound`、`Busy`…） | — | — | |
| 已保存单元格 | **已提交**（committed）/ **已分叉**（forked）/ **已合并**（merged）/ **已发布**（published）/ **无变化**（unchanged），以及版本的短 id（悬停显示完整 id） | — | — | 调用没保存任何东西时显示 **—** |
| 会话单元格 | 会话标题，以及 **第 *n* 轮 · 工具调用**（turn *n* · tool call）、**第 *n* 轮 · 轮次检查点**（turn *n* · turn checkpoint）、**主机保存**（host save）、**引用**（references） | 点标题 → 打开那个对话 | — | 不是 DSH 发起的调用显示 **—** |
| **加载更早的调用**（Load older calls） | | 按下 → 再读 100 条 | `home log --cursor` (读取) | **已加载全部调用。**（All calls loaded.）；**已跳过 *n* 行无法读取的记录。**（*n* unreadable lines were skipped.） |
| 标签筛选块 | **会话 *ef98…* 的调用**（Calls of session *ef98…*，会话 id 去掉了 DSH 的 `session-` 前缀）或 **调用 *id* 的调用**（Calls of call *id*） | **×**（**显示全部调用**（Show all calls））→ 去掉这个筛选 | — | 只在页面是为某个会话、检查点或调用打开时出现。一条都没有时显示 **这些调用不在本机的日志中**（These calls are not in this machine’s log） |

![只看一个会话的日志页](../img/logs-session.png)

为一个会话打开时（1，从 **此会话的调用**（Calls of this session）进来），页面会清掉其他所有筛选，
所以 **仅写入**（Writes only）（2）是关着的，读取和被拒绝的调用也都会显示。这个示例会话一共有 10 次调用：
分支分叉（**第 0 轮 · 起点**（turn 0 · base）），DSH 自己的 `sandbox get`（在创建 Sandbox 之前，所以被拒绝，
**NotFound**），`sandbox create` 和 `workspace mount`（**主机保存**（host save）），之后每一轮各一次
`workspace exec`（智能体的工具调用）和一次 `workspace commit`（轮次检查点）。

## 调用详情

![一次调用的详情，带编号标注](../img/logs-detail.png)

| 组件 | 显示内容 | 操作 → 结果 | LayerFS 调用 | 状态 |
|---|---|---|---|---|
| 头部 | 调用（`workspace commit`）和它的结果；**第 *i* / *n* 条**（*i* of *n*） | ↑ / ↓（1）或方向键 → 表格里的上一条 / 下一条调用。**×** 或 Escape → 回到那一行 | — | |
| **命令**（Command） | 完整命令 | **复制**（Copy）（2）→ 命令复制到剪贴板 | — | **已复制**（Copied） |
| **详情**（Details）（3） | **开始时间**（Started）、**耗时**（Duration）、**客户端**（Client）、**结果**（Outcome）、**结果类型**（Result，`committed`）、**保存的版本**（Saved version，**新建**（created）/ **无变化**（unchanged）/ **引用**（references），以及完整 id）、**标签**（Tag）、**会话**（Session，标题、轮次和检查点） | 版本和标签旁边各有 **复制**（Copy） | — | |
| **打开会话**（Open session）（4） | | 按下 → 打开那个对话 | — | 只有 DSH 发起的调用才有 |
| **此检查点的调用**（Calls of this checkpoint）/ **带此标签的调用**（Calls with this tag）（5） | | 按下 → 表格只显示带这个标签的调用 | `home log --tag` (读取) | |
| **此会话的调用**（Calls of this session）（6） | | 按下 → 表格只显示这个会话的调用 | `home log --tag dsh:<session>:` (读取) | |
| **输出**（Output） | **退出码**（Exit code）、**stdout** 和 **stderr** 的大小，每个流都有 **复制**（Copy），JSON 会标出来；被拒绝的调用显示错误码、消息、**修复：**（Fix:）和 **拒绝详情**（Refusal details） | 输出读不出来时有 **重试**（Retry） | `logs.output`（没有 LayerFS 调用：DSH 为自己发起的调用保留的输出） | **正在读取输出…**（Loading output…）；**此调用没有输出。**（The call printed nothing.）；**已截断：…**（Truncated: …） |

![一次没有保留输出的调用](../img/logs-output.png)

其他客户端发起的调用、较早的调用，以及重置之后，都会显示 **此调用没有保留输出**（No output kept for this call）：
*只保留本机通过 DSH 发起的调用的输出，且只保留最近的；重置会清除它们。LayerFS 自身的调用日志从不保存输出。*

## 变体

- **LayerFS 的调用日志**（`home log`，本服务器用的就是它）：列出这个主目录的所有客户端，重置会为所有客户端清空它。
- **插件的替代日志**（运行时没有 `home log` 时）：只有通过 DSH 发起的调用，重置也只清这些。
- **导入的会话：** 按会话标签筛选时，随会话一起保存在 LayerFS 里的调用（它的回执）排在本机记录之后。

## 在这里打开的对话框

| 对话框 | 打开方式 | 作用 | LayerFS 调用 | 撤销 |
|---|---|---|---|---|
| **重置调用日志？**（Reset the call log?） | **重置…**（Reset…） | *将为使用此 LayerFS 主目录的所有客户端（DSH、LayerFS Connect、终端和脚本）清除调用历史。检查点和已保存的版本会保留。*（用替代日志时：*将删除本机通过 DSH 记录的所有调用…*）。**取消**（Cancel）· **重置**（Reset）→ **已删除 *n* 条调用。**（*n* calls removed.） | `logs.reset` → `home clear-log`（或替代日志文件），以及保留的输出 | 无法撤销：历史已经没了 |
| 调用详情 | 表格中的一行 | 见上文 | — | |

![重置调用日志？](../img/logs-reset.png)

日志为空时 **重置…**（Reset…）不可用。这是一个[确认对话框](components.md#confirm-dialog)。

## 状态与提示

- **结果**（Outcome）里出现红色错误码：LayerFS 拒绝了这次调用。打开这一行，可以看到拒绝消息和 **修复：**（Fix:）。
- **这些调用不在本机的日志中**（These calls are not in this machine’s log）：这个会话是从另一台机器导入的，
  或者之后日志被重置过。
- **没有符合筛选条件的调用**（No calls match these filters）：修改或清除筛选条件。

## 下一步

- [对话](conversation.md)：从会话自己的角度看这些调用（轨迹，以及 Sandbox 图里的活动）。
- [诊断](health.md)：检查被拒绝的调用指向的对象。
- [cli-map.md](cli-map.md)：哪次点击会跑哪条调用。

## 源码

`src/client/logs/`：`LogsPage.tsx`、`CallDetail.tsx`、`Command.tsx`、`format.ts`、
`request.ts`（为某个标签打开页面）、`session-words.ts`、`locale.ts`。主机：
`src/host/logs/index.ts`（`logs.list`、`logs.reset`、`logs.output`、`logs` 事件），
`src/host/logs/source.ts`（`home log` 或替代日志），`src/host/layerfs/call-log.ts`
（替代日志），`src/host/layerfs/call-output.ts`（保留的输出）。
