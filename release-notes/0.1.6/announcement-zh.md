# LayerFS v0.1.6 中文发布公告（社交文案）

> **Status:** post-tag social copy. Written after the `v0.1.6` tag was cut, so this
> file is **not** part of the tag tree; every number in it is copied from the
> [release record](README.md), [verification](verification.md) and
> [closeout](benchmark-closeout.md). 文案风格参照用户提供的中文更新日志体例。

## 长文版

```text
更新日志 2026-09-16
版本 0.1.6

✨ 新功能

- 工作区实时状态迁入沙箱：容器侧 owner 持有可变的命名空间、文件数据与每个挂载私有的 payload backing；宿主只保留规范构造、SQLite Store 与发布，直到 Commit 才接收可变状态传输。
- Commit 路径不再有暂停/静默步骤：FREEZE/RESUME 仅作为无分发处理器的 wire 常量保留，56/56 个 SDK 编辑行实测 commit_pause_fence_ns = 0，工作区全程连续。
- 构造默认且强制单 worker：construction_worker_limit() 与它驱动的规范构造都是单生产者，每次运行都导出 LAYERFS_CONSTRUCTION_WORKERS=1（init_namespace 是唯一例外）。
- 沙箱 spool 改为有界常驻窗口：沙箱自身常驻内存从约 500 MiB 降到 ≤ 2.6 MiB，测量阶段不再由自己的近期写入从页面缓存里买单。
- 扩充注册基准覆盖：F4 compact 分支控制、F5 namespace-inode 历史、F6 historical_access（边界前/后、inode 前/后、fork point、divergent head），以及三个声明式扩展用例（四工作区控制 + 两个穷尽重放）。

🐛 修复与改进

- 修复 HN orchestrator 的阶段计数器与 oracle 断言不一致：helper 观察值与声明值现在逐项对齐，阶段 2 明确定义为仅宿主计数器。
- 修正 boundary-cycle 的交换方向：较高文件的字节应当下移（(131071,131073) ↔ (131072,131072)），两个 K 档用例现按修正后的期望独立验证。
- 重写去重 transcript 期望：跨表示类重编码的路径被如实转写，不匹配时同时打印路径与两侧分解，而不是只报一句不一致。
- 修复 fixture 准备的缓存键与 owner marker 写入顺序，消除 "host prepared Store content mismatch" 的假失败。
- 消除验证器冗余：命名空间遍历不再按声明范围重建、共享 recipe 子树只校验一次、同一 file-state 只做一次认证读取——L500 K100 的验证因此能在声明的上限内完成，oracle 与覆盖范围没有削减。
- 修复 6 个 workspace_reliability 故障注入证明，现为 27/27 PASS；三处修改全在验证路径，不在产品。

📦 兼容性

- **Store 格式没有变化**：SCHEMA_VERSION 仍为 10，自 v0.1.5 以来没有 schema 或静态 SQL 变更；layerfs-content 与 layerfs-layerstack-store 与 v0.1.5 逐字节一致，ObjectId 域、分块、content root 与 Commit 推导均未改变。
- 公开 CLI 未变（layerfs --version 打印 layerfs 0.1.6）；SDK 仅在 test-instrumentation 之后新增两个方法；daemon 协议为增量：挂载请求携带该工作区的快照 backing root。
- 混合版本实时会话不受支持：v0.1.6 的沙箱 owner 与 v0.1.5 的宿主不共享实时工作区，请成套匹配 SDK / CLI / daemon / 运行时。

📊 验证与已知代价（照实公开，不重新标注）

- 36 个用例（33 常规 + 3 声明扩展），seed 1，每个用例每模式一个样本：28 个性能回执（25 PASS + 3 声明例外）与 36 个独立验证全部 PASS，清理全部 PASS，没有 FAIL / TIMEOUT / NOT_RUN。
- 超过 15 s 家族目标的行照实公布：性能 16.492 s、18.312 s、15.760 s（落在声明的 60 s 允许内）；验证 23.10 s（声明的 30 s 上限）；两个仅验证的穷尽重放 15.014 s 与 59.326 s（冻结的 120 s / 300 s 看门狗）。
- 单 worker 带来的材料性回归 1.50–1.65×（去重/CDC 构造）由 owner 接受并保留在记录中；冷启动 namespace-100000 Init 目标由 owner 豁免（4.986 s 对 2.7 s）。
- 内核脏的共享 mmap 仍不会被 Commit 捕获；沙箱进程内存在该测量环境下不可测；耐力（600 s 持续证明）未认证。

平台支持：LayerFS 0.1.6 是源码形式的 Developer Preview，不承诺崩溃/掉电持久性，没有 crates.io 包、预编译可执行文件或公开运行时镜像。
发布页：https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6
```

## 精简版（单帖，约 250 权重，可直接发）

```text
LayerFS 0.1.6 发布 🚀

工作区实时状态搬进沙箱，Commit 不再暂停/静默；构造固定单 worker，沙箱常驻内存 500 MiB → ≤ 2.6 MiB。

Store 格式未变（schema 10）。36 个用例：28 个性能回执 + 36 个独立验证全部 PASS，例外与豁免照实公布。
https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6
```

## 精简版（三帖串）

```text
1/3 LayerFS 0.1.6 发布 🚀 工作区的实时可变状态从宿主搬进沙箱：容器侧 owner 持有命名空间、文件数据与私有 payload backing，宿主保留规范构造、SQLite Store 与发布，并且只在 Commit 时接收可变状态传输。Commit 路径不再有暂停/静默步骤。

2/3 代价与边界都照实说：构造固定单 worker（去重/CDC 构造 1.50–1.65× 回归由 owner 接受），沙箱 spool 有界常驻 ≤ 2.6 MiB，传输按存储读取计价（2.1 GiB/s）而不是缓存热度（19 GB/s）。Store 格式未变：schema 10，与 v0.1.5 逐字节一致，混合版本实时会话不支持。

3/3 验证：36 个用例、seed 1、每模式一个样本——28 个性能回执（25 PASS + 3 声明例外）与 36 个独立验证全部 PASS，清理全部 PASS。超 15 s 目标的行、30 s 验证例外、两个穷尽重放的 120 s/300 s 看门狗，以及所有豁免都写在发布记录里。
https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.6
```
