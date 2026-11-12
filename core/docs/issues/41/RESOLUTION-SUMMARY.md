# LayerFS GC 三问题解决方案汇总

## 📋 问题总述

从 `LayerFS-GC-spec.md` 和 `gc-42-assessment.md` 识别出的三个关键设计问题：

| ID | 类别 | 问题 | 影响 | 状态 |
|----|------|------|------|------|
| **C8b** | 新代码 | SQLite 写者围栏（inode generation 追踪） | P0 风险：silent data loss | 需实现 |
| **N3** | Schema 变更 | 逻辑删除表示（layer_stack / branch） | 属 #42 acceptance point | 需实现 |
| **C9** | API 合约 | HistoryCatalog 8 impl 维护约束 | 降低维护成本 | 需遵循 |

---

## 🔧 C8b：SQLite 写者围栏（Pre-swap Writer Fence）

### 问题本质
Compact 时 Store 文件 rename 交换后，pre-swap 写者的旧 fd 仍能写入被 retire 的 inode，新 Store 无法观测 → **静默数据丢失**。

### 解决方案
**代际机制（incarnation protocol）**：
- Store 持有 `incarnation: u64`，每次 compact install 时递增
- 写入操作在 admission 时检查：`writer_gen_at_acquire < current_gen` → `Conflict`
- 两道关：install 前 catalog_incarnation 增加 + 每个 write 在打开时校验

### 关键代码位置
```
store/identity.rs → StoreGeneration 结构
store/sqlite/writer_admission.rs → 新建，admission check 逻辑
backend/sqlite/session/write_path.rs → 集成 check
persistence/history/catalog.rs → install 时 incarnation 增加
```

### 实现顺序（5步）
1. 添加 `StoreGeneration` 结构 (store/identity.rs)
2. 新建 `writer_admission.rs`
3. 修改 write_path.rs 集成 check
4. 修改 catalog.rs compact install
5. 并发测试（compact + write）

---

## 📊 N3：逻辑删除的表示（Soft Delete Visibility）

### 问题本质
无可见性列，无法实现「软删除」（保留行但隐藏），Name reuse / Branch history 保护做不了。

### 解决方案
**一次 schema v3.5 → v3.6 升级**：
- `layer_stack` → 添加 `deleted_ms` (nullable BIGINT)
- `branch` → 添加 `removed_ms` (nullable BIGINT)
- 索引改为部分索引：`WHERE deleted_ms IS NULL` 和 `WHERE removed_ms IS NULL`
- 删除操作改为 UPDATE SET deleted_ms/removed_ms，不再 DELETE

### 关键 SQL 变更
```sql
-- Old: 全局唯一
CREATE UNIQUE INDEX layer_stack_names ON layer_stack(name)

-- New: 部分唯一（允许多个 tombstone）
CREATE UNIQUE INDEX layer_stack_names 
  ON layer_stack(name) WHERE deleted_ms IS NULL
```

### 实现顺序（6步）
1. 修改 schema 定义（sqlite + postgresql）
2. 新建 migrate.rs（v3.5 → v3.6）
3. 修改 catalog.rs 所有查询加可见性条件
4. 修改 remove.rs：DELETE → UPDATE SET deleted_ms
5. 修改 API remove → delete 语义
6. 迁移测试（旧 DB → 新 schema 可见性验证）

---

## 🧩 C9：HistoryCatalog 扩展约束

### 问题本质
32 个方法 × 8 实现 = 维护爆炸。新增 GC 方法若无默认体，每个都要改 8 处。同时禁止新增 HistoryError 变体（两个通配映射点会静默降级）。

### 解决方案
**约定性设计**：

1. **新 GC 方法必须有 `Unsupported` default 实现体**
   ```rust
   pub trait HistoryCatalog {
       fn mark_branch_removed(...) -> Result<()> {
           Err(HistoryError::Unsupported("mark_branch_removed".to_string()))
       }
   }
   ```
   - 内存 / test double 自动继承，无需改动
   - 真实后端 (SQLite / PostgreSQL) 在 impl 块覆盖

2. **禁止新增 HistoryError 变体**
   - 已有 6 个变体（Unsupported, StaleStore, Conflict, NotFound, InvalidInput, Internal）足矣
   - 细分错误用 String detail：`Conflict("branch_removed_cannot_fork")`

### 关键代码位置
```
core/crates/layerfs-history/src/contract/catalog.rs → 4 个 GC 方法 default
backend/sqlite/src/catalog.rs → override
backend/postgresql/src/catalog.rs → override
core/crates/layerfs-history/src/contract/error.rs → 8 个变体（冻结）
```

### 新增 GC 方法（4个）
- `mark_branch_removed(branch_id, removed_ms) -> Result<()>`
- `mark_project_deleted(stack_id, deleted_ms) -> Result<()>`
- `get_retained_roots() -> Result<RetainedRoots>`
- `remove_unreachable_history(keep_node_ids) -> Result<RemovalStats>`

### 实现顺序（5步）
1. 在 trait 中添加 4 个方法 + Unsupported default
2. SQLite impl 覆盖 + 实现
3. PostgreSQL impl 覆盖 + 实现
4. 运行单元测试（test double 不需改动）
5. 集成测试验证 default Unsupported

---

## 📈 Excalidraw 架构图与时序图

已生成三个可视化设计图：

### 1. C8b - Writer Fence Sequence Diagram
**链接**：https://app.excalidraw.com/s/9Zq6XgRe98z/5fb5lT5yDKY

展示：Writer → Admission → Store generation 校验 → Compact 并发场景 → Conflict 触发

### 2. N3 - Schema Migration Diagram
**链接**：https://app.excalidraw.com/s/9Zq6XgRe98z/1dZkWEGOgA7

展示：v3.5 硬删除 → 迁移步骤 → v3.6 部分索引软删除

### 3. C9 - HistoryCatalog Architecture
**链接**：https://app.excalidraw.com/s/9Zq6XgRe98z/8ZPm9UyP100

展示：32 方法 trait → 8 impl (2 real + 6 test) → 4 GC 方法 default pattern

---

## ✅ 集成验证清单

| 问题 | 关键验证 | 预期结果 |
|------|--------|--------|
| C8b | 并发 compact + write（pre-swap） | Conflict 正确触发，无 silent loss |
| N3 | 旧 DB 迁移 + list 可见性 | 软删除行隐藏，name 可重用 |
| C9 | 运行现有单元测试 | 8 impl 无任何改动（继承 Unsupported） |

---

## 🎯 决策点（open questions）

| ID | 项目 | 建议 | 所有权 |
|----|------|------|--------|
| D1 | BranchId generation 编码 | 冻结：identity_format vs 独立列 | 产品 |
| D3 | PostgreSQL 围栏方式 | 确认：advisory lock vs FOR UPDATE vs schema lock | 产品 |
| D6 | 时间戳精度 | 建议：nullable BIGINT (ms) | 产品 |
| D7 | 源分支删除权限 | 建议：允许（fork 保留所用） | 产品 |

---

## 📁 交付物

- ✅ `C8b-N3-C9-Resolution-Plan.md` (17 KB，560 行）
  - 详细实现路径、代码示例、关键位置
  - 每个问题的推荐实现顺序

- ✅ 三个 Excalidraw 架构图与时序图
  - C8b Writer Fence Sequence
  - N3 Schema Migration Flow
  - C9 HistoryCatalog Architecture

---

## 🚀 后续行动

1. **产品确认** D1, D3, D6, D7 决策点
2. **开发顺序**（建议）
   - Phase 1: N3 schema 迁移（基础设施）
   - Phase 2: C8b writer fence（风险缓解）
   - Phase 3: C9 GC 方法实现（功能完整）
3. **测试覆盖**
   - 单元测试：每个模块的代际、可见性、错误映射
   - 集成测试：schema upgrade，并发 compact+write
   - 端到端：GC 完整流程验证

