# Windows：占位（尚未实现）

状态：占位。选到 `Runtime::Windows` 时返回既有的“不支持此平台”错误
（`RuntimeError::Unsupported`），不 panic、不新建公开错误码、不产生任何副作用。

## 现在有什么

- `layerfs_sandbox::Runtime` 的四个取值：`Docker`、`MacosNative`、
  `LinuxNative`、`Windows`。前三个已有后端，Windows 只有这个变体。
- `Runtime::{capabilities, local_directory, launch, reopen, reopen_stopped}`
  在 Windows 上全部返回 `Unsupported`；`Sandbox` 句柄没有 Windows 分支，
  因为 Windows 上不可能启动出实例。
- `layerfs-sandbox` 的 `backend` 只在 unix 下编译，`layerfs-node` 也只用
  unix 系统调用，所以今天在 Windows 主机上选不到这个变体：它存在的意义是
  把“没有 Windows 宿主运行时”写进类型，而不是让调用方自己判平台。

## 为什么不做“Windows 本地进程”后端

macOS 原生的前提是：daemon 以本用户身份跑 NFS 回环服务，命令以同一身份
运行，一切在同一个内核里。Windows 上没有等价物：LayerFS 的 FUSE 服务端是
Linux 内核特性，Store 的 WAL 与目录锁只在一个内核内协调（见 #352 直接
Store 访问的安全条件），命令身份、挂载可见性和进程组语义都要另建一套。
因此 Windows 的路线不是“再写一个本地后端”，而是把 Linux 侧的能力搬到一个
Linux 宿主里。

## 计划方向

两条路线，都由 #515 的 placement 设置表达，而不是在 Windows 本机上跑 daemon：

1. **WSL2 分发（近期）**：Windows 上的集成适配器把 SDK 调用转发给
   每个 LayerFS home 一个的、仅回环且带密钥的 worker；worker 在一个选定的
   WSL2 Linux 分发里直接用真实 SDK 打开共享 Store（ext4/xfs/btrfs），
   并用该分发的 Docker Engine 起 Sandbox daemon。Windows 只拥有 UI、绑定和
   trace，Linux 拥有本地句柄和项目发现。这是仓库根 `AGENTS.md` 已记录的
   Windows 例外，规则不变：只连回环、每次调用只尝试一次、丢失的回复不重放、
  结果不确定的 Init 保留其 Linux home 和卷。
2. **Hyper-V / VM 方向（后续）**：每个 Store 一个 Linux VM，Sandbox 是 VM 内
   的命名空间；宿主侧只通过一条私有流连到 VM 内的 worker。见 #515 §6
   （`vm:<provider>` placement 与各 provider 的 VM/磁盘/私有流对照表）和
   #512 的实验结论。

## 实现时要先定的事

- placement 与后端选择的设置名（#515 V9 提议 `LAYERFS_PLACEMENT` 与
  `LAYERFS_SANDBOX_BACKEND`，只读 `LAYERFS_*`；集成自己的前缀由集成映射）。
- Windows 上 `deploy.sandbox`、`deploy.localDir` 等现有设置的语义：#515
  要求 product SDK 与 daemon 不为 Windows 改动，Windows 差异留在适配器里。
- 到那时 `Runtime` 是否新增一个持有 worker 端点的变体（那时才需要数据），
  以及记录里如何标注该 Sandbox 由哪个运行时创建（无字段即 Docker）。
