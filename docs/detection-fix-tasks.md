# 任务清单：layerfs 检测与身份修复（可在 Dell 上执行）

背景与判定依据见 `https://github.com/Ephemeral-AI-Lab/layerfs/issues/752`（三条评论）。
本文是施工单，按顺序执行，**每一步必须 `cargo check` 或 `cargo test` 通过后才进下一步**。

---

## 0. 环境与分支

```bash
cd /home/dell/src/tokenaissance/layerfs          # 已在 ed9118963（= upstream/layerfs_v2_0_0）
git fetch origin && git fetch upstream 2>/dev/null || true
git switch -c fix/project-manifest-detection upstream/layerfs_v2_0_0 || git switch fix/project-manifest-detection
```

先确认基线：

```bash
git log --oneline -1            # 期望 ed9118963 …
git status --porcelain          # 期望空（有本地改动先 stash）
```

**守卫**：所有验证性命令都用宿主编译器与仓库内 CLI，**不要**用 `/layerfs/bundle/layerfs`（它是 guest 版，`machine.*` 会报 `machine.start runs on the host`）。

```bash
export CARGO_NET_OFFLINE=true    # 若网络不通；依赖缓存应已足够
```

---

## 1. `c2` 机器身份：名字从完整 home id 推导

**目标**：消除 8-hex（32 bit）命名空间，并让 `provider_name` 校验而不是信任记录。

**文件**
- `control/crates/layerfs-home/src/names.rs:27-29`
- `control/crates/layerfs-machine/src/record.rs:143-149`

**步骤**
1. 保留 `provider_prefix` 不变；**新增** `legacy_prefix(home_id) -> String`（沿用 id8 逻辑），并给两者写 doc 注释说明新旧关系。
2. 把 `provider_prefix` 改为 16 hex（`&home_id[..16.min(len)]`）。
3. 给 `provider_name()` 增加 `home_id: &str` 参数：记录值等于 `provider_prefix` 或 `legacy_prefix` 拼出的名字才返回，否则 `refused("InvalidInput", …)`，消息里同时给出记录名与期望名。
4. 更新调用点：`grep -rn "provider_name()" control api` 逐个传当前 `home_id`。

**完成判据**
```bash
cargo check -p layerfs-home -p layerfs-machine
cargo test  -p layerfs-machine
```
新增单元测试：
- 两个 home id 仅前 8 位相同 → `provider_prefix` 不同
- `record.vm_name = layerfs-<别的 id>-docker` → `provider_name` 返回 `InvalidInput`
- `vm_name` 为旧 id8 名 → 仍然通过（兼容）

---

## 2. `c3` 数据卷：打标签 + 报告孤儿卷

**目标**：让"这块盘属于谁"可从卷本身判断；孤儿卷能被发现。

**文件**
- `control/crates/layerfs-platform/src/docker/power.rs:86-95, 102, 140`
- `control/crates/layerfs-control/src/doctor/machine_target.rs:570-595`
- 可复用 `control/crates/layerfs-control/src/inventory.rs`

**步骤**
1. 创建容器时给数据卷加标签：`--label layerfs.home=<home id>`、`--label layerfs.machine=<name>`（Docker 允许对 `volume create` 设标签；`docker run -v name:path` 自动建卷时无法带标签，所以改为**先 `docker volume create --label` 再挂载**）。
2. `machine_target.rs` 的机器检查新增一项：列出带 `layerfs.home` 标签、但当前 home 里**没有对应 machine 记录**的卷 → `checks.fail`，fix 文案给出检查命令。
3. 卷存在但标签缺失（老卷）→ `warning`，fix 提示可 `docker volume inspect` 人工确认。

**完成判据**
```bash
cargo check -p layerfs-platform -p layerfs-control
cargo test  -p layerfs-control
```
测试：
- 一个带错误 `layerfs.home` 标签的卷 → doctor 报 finding
- 无标签卷 → warning，不是 error

---

## 3. `c4` `home.layout` 必须交叉校验 id

**目标**：伪造的 `layerfs.json` 不能再通过唯一的 home 检查（本次事故就是这样骗过系统的）。

**文件**
- `control/crates/layerfs-home/src/home/layout.rs:120-126`
- `control/crates/layerfs-control/src/doctor/host_target.rs:53-72`

**步骤**
1. `layout.rs` 保留 32-hex 语法校验，**新增**语义校验函数 `id_conflicts(home, id) -> Option<String>`：把 id 与
   - 现有 `machine/*/record.json` 的 `vm_name`（期望前缀 `layerfs-<id[..16]>-` 或 `<id[..8]>-`）
   - 备份桶对象的 `layerfs-exported-from`（若可读到）
   - 既有沙箱记录里的守护进程名前缀
   三者比对，任一不匹配即返回冲突描述。
2. `host_target.rs` 的 `LAYOUT` 检查改为：语法通过但语义冲突 → `checks.fail`，消息里写明"这个 id 与本机记录不符"。

**完成判据**：新增 fixture —— 复制一份 home、替换 `layerfs.json` 的 id，`doctor` 必须报错。

---

## 4. `c5` 按内容识别 worker root

**目标**：正确的拒绝（"这是 worker root，不是 home"）变得可达。

**文件**
- `control/crates/layerfs-home/src/home/guest.rs:33-36, 50-63`
- `control/crates/layerfs-home/src/home/layout.rs`（判定入口）
- `docs/versioned/2.0.0/home.md:74, 245-251`

**步骤**
1. 新增 `worker_root_shape(dir) -> bool`：同时满足
   - 有 `store/` 或 `sandbox/` 或 `machine/native/run/`
   - 有 `incoming/`（`init-*` 子目录）或 `log/worker.log`
   - **没有** `trace/`、**没有** `backup/`、**没有** `layerfs.json`
2. `detect()` 在 `own_only == false` 时先判 `worker_root_shape`：为真 → 新变体 `Layout::WorkerRoot`（或复用 `Foreign` 但换文案），给出的 fix 是"用 `LAYERFS_HOME` 指向你的 home，通过 machine 子命令操作这块盘"，**不是** `mv`。
3. `home.md` 把 `.layout-3` 段落标注为**历史机制**（本 build 不写）。

**完成判据**：fixture（目录形态如本次事故）→ 报 `worker root`，且**不**出现 `mv` 建议。

---

## 5. `c6` 真 home 的接管：`own_only` 允许表 + `home init --adopt`

**目标**：真 home 丢了 `layerfs.json` 时可修复，而不是被判 `Foreign` 后 `mv` 走人。

**文件**
- `control/crates/layerfs-home/src/home/layout.rs:72-104`
- `control/crates/layerfs-control/src/home.rs:59-61, 114-121`
- `tests/home_layout.rs:137-180`、`tests/doctor_host.rs:110-150`

**步骤**
1. 允许表加入本 build 自己的目录：`store machine sandbox backup secret trace log generation incoming`。
2. `own_only == false` 且**不是** worker root、**没有** `layerfs.json` → 新变体 `Layout::DataWithoutMarker`。
3. 新增 `home init --adopt`：写 `layerfs.json`（**保留现有数据**，id 由第 3 步的交叉校验给出或新建并标 warning），随后跑一次只读审计并报告缺项。
4. `foreign_fix()` 文案调整：先给 adopt，`mv` 降为"确认不要这些数据时"的选项。
5. 同步更新上面两个测试文件与被 `docs/versioned/2.0.0/home.md:300-312` 固化的描述。

**完成判据**
```bash
cargo test -p layerfs-control --test home_layout --test doctor_host
```
- fixture：有 `store/`、无 marker → `DataWithoutMarker`，`home get` 给出 adopt 建议
- fixture：worker root 形态 → **不**进入 adopt 分支（第 4 步的判定优先）

---

## 6. `c7` 项目 record 必须可解码

**目标**：`project doctor` 与 `project get` 不再对不可用的项目说 healthy。

**文件**
- `control/crates/layerfs-store/src/project/record.rs:36-39, 86-95`
- `control/crates/layerfs-control/src/doctor/project_target.rs:64-65`
- `control/crates/layerfs-control/src/doctor/kinds.rs:333-335, 348-349`
- `control/crates/layerfs-store/src/project/discovery.rs:280`

**步骤**
1. `ProjectRecord` 新增 `complete(&self) -> Result<(), Reason>`：`manifest` 存在且 `StoreManifest::decode` 成功；`installed` 存在时同样要求可解码。
2. `project_target.rs` 的 `RECORD` 检查：`complete()` 失败 → `checks.fail`，fix 指向第 7 步的修复命令。
3. `kinds.rs` 的项目状态改为由 `ProjectRecord::read` + `complete()` 推导（`ready` / `broken` / `failed`），不再硬编码。
4. `discovery.rs:280` 的错误码从 `NoSuchProject` 改为语义正确的码（如 `Incomplete`），消息保留原句 "no complete project manifest" 以便日志检索，fix 给出修复入口。

**完成判据**：fixture（record 无 manifest）→ `project doctor` 失败、`project get` 显示 `broken`、`sandbox create` 的报错带 fix。

---

## 7. `c8` 可脚本化失败 + Init 计数校验 + 小项

**目标**：检查能真正阻断流水线；并发/中断不再产生静默不一致。

**文件**
- `api/crates/layerfs-cli/src/run.rs:205-211`、`output.rs:120-123`
- `control/crates/layerfs-control/src/doctor/{workspace_target.rs:129-130, host_capacity.rs:14-20, backup_target.rs:92-97}`
- `control/crates/layerfs-store/src/project/{add.rs:77-106, 178-190}`、`op.rs:37-53`、`staged.rs:91-100`
- `control/crates/layerfs-control/src/doctor/scan.rs:254-267, 325-343`

**步骤**
1. `doctor` 的 `ok:false` → 退出码非零（新增 `--fail-on error|never`，默认 `error`；若维护者坚持 AGENTS.md 的"never gates"，则默认 `never` 并在 `--help` 与文档中写明）。
2. `project.workspace`：`workspaces(dir).unwrap_or_default()` 改为 `Result`，读不到 → `checks.unknown`（不是"没有 workspace"）。
3. `host_capacity`：home 不存在时不要量祖先目录，直接 `unknown`。
4. `backup_target`：每个子检查各自计时，别共用一个 deadline。
5. `project init`：把 `send::measure` 的文件数/字节数与 Store 收下的 `records` 计数比较，不等 → Init 失败。
6. **孤儿项目目录**：`scan.rs` 对"目录在、record 缺"报 finding（`Incomplete`）；`NameTaken` 时区分"有记录"与"未完成"，后者提示 `project prune <store>/<name>`；实现 `prune`（仅当无 record 时允许删目录）。

**完成判据**
```bash
cargo test -p layerfs-control -p layerfs-store
cargo clippy --workspace -- -D warnings   # 若仓库启用
```
- fixture：人工 `mkdir store/dsh/project/orphan` → doctor 报 `Incomplete`，`project init` 同名给出 prune 提示
- fixture：Init 计数不一致（mock 少收一条）→ 失败

---

## 8. 收尾

```bash
cargo fmt --all
cargo test --workspace
git add -A && git commit -m "fix(home,machine,doctor): identity, worker roots and project completeness"
git push origin fix/project-manifest-detection
```

PR 目标：`Ephemeral-AI-Lab/layerfs`，base `layerfs_v2_0_0`，head `tokenaissance:fix/project-manifest-detection`。

PR 描述里按 `c2…c8` 分节，并附"如何验证"（fixture 清单 + 主机实测）。

---

## 纪律（本次事故的教训，逐条照做）

1. **不跳过 `cargo check`**：每一步改完立刻编译。
2. **测试与文档同 PR**：`tests/home_layout.rs`、`tests/doctor_host.rs`、`docs/versioned/2.0.0/home.md` 固化了当前行为。
3. **绝不在机器的数据盘里写 `layerfs.json`**：那是 worker root；用 `LAYERFS_HOME` 指向真 home。
4. **绝不 `mv` 数据盘**：它是容器挂载点，挪走等于让卷变空。
5. **区分两个 CLI**：宿主机 `/home/dell/layerfs-bin/layerfs`（有 `machine.*`）vs bundle 内 `/layerfs/bundle/layerfs`（guest 版）。
6. **验证只看四项**：`store list` / `project list` / `sandbox create` / `sandbox remove` 全绿才算改动有效。
