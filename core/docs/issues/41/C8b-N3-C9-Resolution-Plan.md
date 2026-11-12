# LayerFS GC 三个关键设计问题解决方案

## 问题背景

基于 `LayerFS-GC-spec.md` 第 5 章「契约」和 `gc-42-assessment.md` 第 4 章「开放问题」，三个问题分别属于不同的实现层级：

---

## C8b：SQLite 写者围栏（pre-swap writer fence）

### 问题表述
在 compact 执行 Store 文件 rename 交换时，pre-swap 写者（在旧 Store fd 上取得的写句柄）仍能写入被 retire 的 inode，而新 Store 看不到这些写入——导致**静默数据丢失**。

现有的 install 前 `(dev, ino)` inode 校验不够。

### 根本原因
- Compact rename 之后，旧 fd 指向的文件系统节点（inode）已被 retire（删除或隐藏）
- 新 Store 无法观测 pre-swap 写者对旧 inode 的写入
- 新 Store 和旧 Store 指向不同的物理数据

### 规格决策（frozen contract §3.8）
**SQLite 必须实现一个写者围栏**：
1. **install 前重读代际**（incarnation / catalog_incarnation）
2. **任何在旧代际上取得的写句柄在 install 后必须失败**
3. **复用现有 `Conflict` 拒绝码**，不新增拒绝码类型

### 实现路径

#### 第一步：识别代际检验点
```
写入操作的关键流程：
  admission (store_id, generation, ...)  
    ↓
  take_write_lock_or_fail(generation check)  ← C8b 第一道关
    ↓
  write operations (sqlite write handles)
    ↓
  commit/flush
    ↓
  install_check(dev, ino, generation)  ← C8b 第二道关（现有）
```

**关键代码位置**：
- `control/crates/layerfs-store/src/identity.rs` — Store 身份、lease、generation 记录
- `control/crates/layerfs-store/src/sqlite/writer_admission.rs` (新建) — 写入准入
- `backend/sqlite/lifecycle/installation.rs` — install 阶段 inode 校验
- `engine/crates/layerfs-persistence/src/history/catalog.rs` — catalog_incarnation 增量

#### 第二步：代际映射设计
```rust
// Store 侧（store/identity.rs）
pub struct StoreGeneration {
    incarnation: u64,  // 每次 compact 尝试 +1（install 前）
    file_inode: (dev_t, ino_t),
    last_writer_generation: u64,  // 最后一次成功写入时的代际
}

// 写入路径（新建 writer_admission.rs）
pub fn writer_admission_check(
    store_id: &str,
    current_gen: u64,
    writer_gen_at_acquire: u64,
) -> Result<WriteHandle, StoreError> {
    if writer_gen_at_acquire < current_gen {
        // 这个写句柄是在旧代际上取得的，compact 后不能用
        Err(StoreError::Conflict(
            format!("writer incarnation mismatch: acquired {}, current {}", 
                    writer_gen_at_acquire, current_gen)
        ))
    } else {
        Ok(WriteHandle { /* ... */ })
    }
}
```

#### 第三步：Compact 流程集成
```
store.compact(store_name):
  stage 1: "Prepare"
    ├─ lock Store (exclusive, no new writers)
    └─ read_generation() → gen_before = catalog_incarnation + 1

  stage 2: "Mark"
    └─ reachability walk (decode_references + ancestry)

  stage 3: "Rebuild"
    └─ new Store file in temp location

  stage 4: "Verify"  
    └─ check roots resolve, pack signatures valid

  stage 5: "Install"
    ├─ catalog_incarnation += 1  ← 代际跳进
    ├─ install_check(new inode) ← 存储层 inode 校验
    └─ atomic rename(temp → live)

  stage 6: "Retire"
    └─ delete old file

  → Any write acquired at gen ≤ gen_before will fail Conflict on admission
```

#### 第四步：SQLite 特定细节
- **准入点**：`backend/sqlite/session/write_path.rs` — 每个 write 打开时检验
- **代际持久化**：`history_meta.incarnation` 或 `store_generation` 表
- **测试**：并发 compact 与 write，verify Conflict 被正确触发

### 推荐实现顺序
1. 在 `store/identity.rs` 添加 `StoreGeneration` 结构
2. 新建 `store/sqlite/writer_admission.rs`，实现 admission check
3. 修改 `backend/sqlite/session/write_path.rs`，在打开写句柄时调用 check
4. 修改 `persistence/history/catalog.rs`，在 compact install 时增加 incarnation
5. 测试：并发写 + compact 场景

---

## N3：逻辑删除的表示（soft delete visibility）

### 问题表述
当前 `branch` 和 `layer_stack` 表无可见性列，无法实现「软删除」（逻辑删除但物理保留行）。

### 规格决策（frozen contract §3.4）
在 **一次 schema 版本升级** 中：
1. 为 `layer_stack` 添加 `deleted_ms` 列（nullable timestamp）
2. 为 `branch` 添加 `removed_ms` 列（nullable timestamp）
3. 将全局唯一索引 `layer_stack_names` 改为 **部分索引** `ON layer_stack(name) WHERE deleted_ms IS NULL`
4. 将 branch 唯一索引改为 **部分索引** `ON branch(layer_stack_id, name) WHERE removed_ms IS NULL`

### 实现路径

#### 第一步：Schema 迁移（两个后端统一）

**SQLite** (`sql/sqlite/history.sql`):
```sql
-- 3.5 → 3.6 migration
ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;

-- 删除非部分索引
DROP INDEX IF EXISTS layer_stack_names;
DROP INDEX IF EXISTS branch_names;

-- 创建部分索引
CREATE UNIQUE INDEX layer_stack_names 
  ON layer_stack(name) WHERE deleted_ms IS NULL;

CREATE UNIQUE INDEX branch_names 
  ON branch(layer_stack_id, name) WHERE removed_ms IS NULL;
```

**PostgreSQL** (`sql/postgresql/schema.sql`):
```sql
ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;

DROP INDEX IF EXISTS layer_stack_names;
DROP INDEX IF EXISTS branch_names;

CREATE UNIQUE INDEX layer_stack_names 
  ON layer_stack(name) WHERE deleted_ms IS NULL;

CREATE UNIQUE INDEX branch_names 
  ON branch(layer_stack_id, name) WHERE removed_ms IS NULL;
```

#### 第二步：可见性查询改造

**原有查询**（物理删除模式）：
```sql
SELECT * FROM layer_stack WHERE name = ?
```

**新查询**（软删除模式）：
```sql
SELECT * FROM layer_stack WHERE name = ? AND deleted_ms IS NULL

-- 或统一宏
#define VISIBLE_STACK "AND deleted_ms IS NULL"
SELECT * FROM layer_stack WHERE name = ? VISIBLE_STACK
```

**列出所有项时**：
```sql
-- 只列举活跃项
SELECT * FROM layer_stack WHERE deleted_ms IS NULL ORDER BY ...

-- 管理界面可选：包含已删除项
SELECT *, (deleted_ms IS NOT NULL) AS is_deleted FROM layer_stack ...
```

#### 第三步：删除操作改为 UPDATE

**原有代码**（`project/remove.rs`, `branch/remove.rs`）：
```rust
// 旧模式：删除行
DELETE FROM branch WHERE id = ?
DELETE FROM layer_stack WHERE id = ?
```

**新代码**：
```rust
// 新模式：标记删除时间
UPDATE layer_stack SET deleted_ms = ? 
  WHERE id = ? AND deleted_ms IS NULL;

UPDATE branch SET removed_ms = ? 
  WHERE id = ? AND removed_ms IS NULL;
```

#### 第四步：编目约束（C9 的反面例子）

**HistoryCatalog** 中的每个查询方法需要在 SQL 生成时自动加可见性条件：
```rust
// engine/crates/layerfs-persistence/src/history/catalog.rs

impl HistoryCatalog for SqliteCatalog {
    fn list_projects(&mut self, stack_id: &LayerStackId) -> Result<Vec<LayerStack>> {
        // 自动包含可见性条件
        self.conn.query("
            SELECT * FROM layer_stack 
            WHERE id = ? AND deleted_ms IS NULL
        ", ...)
    }

    fn list_branches(&mut self, stack_id: &LayerStackId) -> Result<Vec<Branch>> {
        self.conn.query("
            SELECT * FROM branch 
            WHERE layer_stack_id = ? AND removed_ms IS NULL
        ", ...)
    }
}
```

#### 第五步：迁移触发器

需要一个 on-open 迁移：
```rust
// engine/crates/layerfs-persistence/src/history/migrate.rs

pub fn migrate_3_5_to_3_6(conn: &mut Connection) -> Result<()> {
    // 1. 添加列
    conn.execute_batch("
        ALTER TABLE layer_stack ADD COLUMN deleted_ms BIGINT DEFAULT NULL;
        ALTER TABLE branch ADD COLUMN removed_ms BIGINT DEFAULT NULL;
    ")?;
    
    // 2. 重建索引
    conn.execute("DROP INDEX layer_stack_names")?;
    conn.execute("DROP INDEX branch_names")?;
    
    conn.execute("
        CREATE UNIQUE INDEX layer_stack_names 
        ON layer_stack(name) WHERE deleted_ms IS NULL
    ")?;
    
    conn.execute("
        CREATE UNIQUE INDEX branch_names 
        ON branch(layer_stack_id, name) WHERE removed_ms IS NULL
    ")?;
    
    // 3. 更新 history_meta
    conn.execute("UPDATE history_meta SET schema_version = 6")?;
    
    Ok(())
}
```

### 推荐实现顺序
1. 修改 `sql/sqlite/history.sql` 和 `sql/postgresql/schema.sql`
2. 新建 `persistence/history/migrate.rs`，实现迁移逻辑
3. 修改 `persistence/history/catalog.rs`，所有查询加可见性条件
4. 修改 `project/remove.rs` 和 `branch/remove.rs`，改为 UPDATE
5. 修改 `api/project/remove.rs` 和 `api/branch/remove.rs`，改为 `delete` 语义
6. 迁移测试：旧数据库 → 新 schema，verify 可见性正确

---

## C9：HistoryCatalog 约束

### 问题表述
`HistoryCatalog` trait 有 32 个方法，8 个实现体：
- 1 个真实实现（SQLite + PostgreSQL）
- 6 个测试 double（daemon/mock）
- 1 个内存实现

新增 GC 方法若无默认体，8 个 impl 同时要改 → **维护成本爆炸**。

同时禁止新增 `HistoryError` 变体，否则两个映射点使用 `_ => "Failed"` 通配，新变体会静默降级。

### 规格决策（frozen contract §5，§2.3）
1. **所有新增 GC 方法必须有 default `Unsupported` 实现体**
2. **禁止新增 `HistoryError` 变体**，复用现有码
3. **default 体适用于内存/测试 double，真实后端在 impl 块中覆盖**

### 实现路径

#### 第一步：检查现有结构

**HistoryCatalog 位置**：
- `core/crates/layerfs-history/src/contract/catalog.rs`

**错误类型**：
- `core/crates/layerfs-history/src/contract/error.rs` — `HistoryError` enum

**8 个实现体**：
1. `backend/sqlite/src/catalog.rs` — SQLite (真实)
2. `backend/postgresql/src/catalog.rs` — PostgreSQL (真实)
3. `engine/crates/layerfs-store-service/src/mocks/catalog.rs` — test double
4. `core/crates/layerfs-history/src/testing/mock_catalog.rs` — test double
5-8. 其他 daemon/fixture doubles

#### 第二步：检查现有方法统计

```rust
// 统计现有方法
pub trait HistoryCatalog {
    // 22 required（无 default）
    fn create_project(...) -> Result<...>;
    fn get_project(...) -> Result<...>;
    // ... 20 more
    
    // 10 optional（有 default，返回 Unsupported）
    fn list_projects_with_stats(...) -> Result<...> {
        Err(HistoryError::Unsupported("list_projects_with_stats".to_string()))
    }
    // ... 9 more
}
```

#### 第三步：GC 新方法及其默认实现

**需要添加的 4 个 GC 方法**（按 spec §3）：

```rust
// 在 core/crates/layerfs-history/src/contract/catalog.rs

impl HistoryCatalog {
    // ...existing methods...

    // GC: branch removal marking
    fn mark_branch_removed(
        &mut self,
        branch_id: &BranchId,
        removed_ms: i64,
    ) -> Result<()> {
        Err(HistoryError::Unsupported(
            "mark_branch_removed".to_string()
        ))
    }

    // GC: project soft deletion
    fn mark_project_deleted(
        &mut self,
        stack_id: &LayerStackId,
        deleted_ms: i64,
    ) -> Result<()> {
        Err(HistoryError::Unsupported(
            "mark_project_deleted".to_string()
        ))
    }

    // GC: retained roots for reachability walk
    fn get_retained_roots(
        &mut self,
    ) -> Result<RetainedRoots> {
        Err(HistoryError::Unsupported(
            "get_retained_roots".to_string()
        ))
    }

    // GC: compact-phase cleanup queries
    fn remove_unreachable_history(
        &mut self,
        keep_node_ids: &HashSet<NodeId>,
    ) -> Result<RemovalStats> {
        Err(HistoryError::Unsupported(
            "remove_unreachable_history".to_string()
        ))
    }
}
```

#### 第四步：真实后端覆盖实现

**SQLite 实现** (`backend/sqlite/src/catalog.rs`):
```rust
impl HistoryCatalog for SqliteCatalog {
    fn mark_branch_removed(
        &mut self,
        branch_id: &BranchId,
        removed_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE branch SET removed_ms = ? WHERE id = ?",
            params![removed_ms, branch_id.to_bytes()],
        )?;
        Ok(())
    }

    fn mark_project_deleted(
        &mut self,
        stack_id: &LayerStackId,
        deleted_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE layer_stack SET deleted_ms = ? WHERE id = ?",
            params![deleted_ms, stack_id.to_bytes()],
        )?;
        Ok(())
    }

    fn get_retained_roots(&mut self) -> Result<RetainedRoots> {
        let mut roots = RetainedRoots::default();
        
        // Layers from visible stacks
        let mut stmt = self.conn.prepare(
            "SELECT id, root_id FROM layer 
             WHERE layer_stack_id IN (
                 SELECT id FROM layer_stack WHERE deleted_ms IS NULL
             )"
        )?;
        // ...populate roots.layers...
        
        // Commits from visible branches
        let mut stmt = self.conn.prepare(
            "SELECT id, root_id FROM commit 
             WHERE branch_id IN (
                 SELECT id FROM branch WHERE removed_ms IS NULL
             )"
        )?;
        // ...populate roots.commits...
        
        Ok(roots)
    }

    fn remove_unreachable_history(
        &mut self,
        keep_node_ids: &HashSet<NodeId>,
    ) -> Result<RemovalStats> {
        // Mark unreachable commits/layers for removal
        let mut stats = RemovalStats::default();
        
        // Delete unreachable commits
        let deleted_commits = self.conn.execute(
            "DELETE FROM commit 
             WHERE id NOT IN (?1) AND id IN (
                 SELECT id FROM commit WHERE removed_ms IS NOT NULL
             )",
            // ... bind keep_node_ids ...
        )?;
        stats.deleted_commits = deleted_commits;
        
        // Similar for layers, objects, etc.
        
        Ok(stats)
    }
}
```

**PostgreSQL 实现** (`backend/postgresql/src/catalog.rs`) — 类似

#### 第五步：错误类型约束（禁止新增变体）

**现有 HistoryError** (`core/crates/layerfs-history/src/contract/error.rs`):
```rust
#[derive(Debug, Clone)]
pub enum HistoryError {
    Unsupported(String),
    StaleStore,
    Conflict(String),
    NotFound(String),
    InvalidInput(String),
    Internal(String),
    // 禁止添加新变体！
}

// 两个映射点：API 层 + 错误报告
// api/crates/layerfs-api/src/error.rs
impl From<HistoryError> for ApiError {
    fn from(e: HistoryError) -> Self {
        match e {
            HistoryError::Unsupported(s) => ApiError::MethodNotSupported(s),
            HistoryError::StaleStore => ApiError::StaleStore,
            HistoryError::Conflict(s) => ApiError::Conflict(s),
            HistoryError::NotFound(s) => ApiError::NotFound(s),
            HistoryError::InvalidInput(s) => ApiError::InvalidInput(s),
            HistoryError::Internal(s) => ApiError::Internal(s),
            _ => ApiError::Internal("Unmapped HistoryError".to_string()),
            // 注意：若加了新变体，这里的 _ 会静默降级成 Internal，难以调试！
        }
    }
}
```

**改进方案**：在编译时检查
```rust
// Cargo feature: strict_error_mapping
#[cfg(feature = "strict_error_mapping")]
impl From<HistoryError> for ApiError {
    fn from(e: HistoryError) -> Self {
        match e {
            // 显式列举所有变体，无 _ 通配
            HistoryError::Unsupported(s) => ...,
            // ... 会在新增变体时编译失败
        }
    }
}
```

#### 第六步：维护规则文档

**在 spec 或 README 中明确**：
```markdown
## HistoryCatalog 扩展规则

1. **新 GC 方法必须有 `Unsupported` default**
   - 内存/test double 自动获得不支持响应
   - 真实后端在 impl 块覆盖，提供实际实现

2. **禁止新增 HistoryError 变体**
   - 已有 6 个变体足以覆盖所有情况
   - 若需细分错误，复用现有变体 + String 详情
   - 例：`Conflict("branch_removed_cannot_fork")` vs 新 `BranchRemoved`

3. **实现检查清单**
   - [ ] 新方法在 trait 中有 default 实现
   - [ ] SQLite impl 覆盖（backend/sqlite/src/catalog.rs）
   - [ ] PostgreSQL impl 覆盖（backend/postgresql/src/catalog.rs）
   - [ ] 测试 double 不需改动（继承 default）
   - [ ] 单元测试验证 Unsupported 错误码
```

### 推荐实现顺序
1. 在 `core/crates/layerfs-history/src/contract/catalog.rs` 添加 4 个 GC 方法的 default 实现
2. 在 `backend/sqlite/src/catalog.rs` 实现 SQLite 版本
3. 在 `backend/postgresql/src/catalog.rs` 实现 PostgreSQL 版本
4. 运行现有单元测试，确保 test double 和内存实现不需改动
5. 添加集成测试：验证 default Unsupported 被正确返回

---

## 集成验证清单

| 问题 | 状态 | 验证方法 |
|------|------|--------|
| **C8b** | 新代码 | 并发 compact + write，verify Conflict 触发 |
| **N3** | Schema 变更 | 迁移测试：旧 DB → 新 schema，list 可见性正确 |
| **C9** | API 合约 | 检查 8 个 impl，无任何改动（只用 default） |

---

## 开放设计项（decision points）

| ID | 项目 | 状态 | 所有权 |
|---|---|---|---|
| D1 | BranchId 代际编码方式（identity_format vs 独立列） | 建议方案已冻结 | 产品 |
| D3 | PostgreSQL 围栏选择（advisory lock vs FOR UPDATE vs schema lock） | 需确认 | 产品 |
| D6 | 时间戳精度（millisecond vs epoch） | 建议 nullable BIGINT ms | 产品 |
| D7 | 删除源分支是否允许 | 建议允许（fork 保留所用） | 产品 |


---

# C9 深度重设计：用 CCP/CRP 原则指导

## 问题根本原因分析

### 症状
- 每次添加 4 个 GC 方法
- 同时改 8 个实现体：SQLite、PostgreSQL、6 个 Mock
- 维护成本 = 8x

### 根本原因
**违反两个组件原则：**

1. **违反 CCP (Common Closure Principle)**
   - Trait 混合了「核心查询」和「GC 操作」
   - 这两类因不同原因改变（业务逻辑 vs GC 实现）
   - 结果：两个改变原因被迫修改同一个 trait

2. **违反 CRP (Common Reuse Principle)**
   - SQLite + PostgreSQL（生产）需要 GC 方法
   - Mock（测试）不需要 GC 方法
   - 结果：因使用场景不同，却强制 Mock stub GC

## CCP/CRP 解决方案

### Step 1: 分离 Trait（CCP）

```rust
// 核心 trait（稳定，改变少）
pub trait HistoryCatalog {
    fn list_projects(...) -> Result<...>;
    fn get_project(...) -> Result<...>;
    // ... 20+ 非 GC 方法（改变原因：业务逻辑）
}

// GC 扩展 trait（频繁改变，隔离）
pub trait HistoryCatalogGC: HistoryCatalog {
    fn mark_branch_removed(...) -> Result<()>;
    fn mark_project_deleted(...) -> Result<()>;
    fn remove_file_from_history(...) -> Result<()>;
    fn get_retained_roots(...) -> Result<()>;
}
```

**好处:**
- ✅ 核心 trait 稳定，改动少
- ✅ GC trait 改动不影响其他代码
- ✅ 因不同原因改变的代码得到隔离

### Step 2: 按使用场景组织（CRP）

```
core/crates/layerfs-history/src/
├── contract/
│   ├── catalog.rs              # HistoryCatalog
│   └── catalog_gc.rs           # HistoryCatalogGC ← NEW
│
├── backend/sqlite/
│   ├── catalog.rs              # impl HistoryCatalog
│   └── catalog_gc.rs           # impl HistoryCatalogGC ← NEW
│
├── backend/postgresql/
│   ├── catalog.rs              # impl HistoryCatalog
│   └── catalog_gc.rs           # impl HistoryCatalogGC ← NEW
│
└── testing/
    ├── mock_catalog.rs         # impl HistoryCatalog (NO GC)
    ├── daemon_mock.rs          # impl HistoryCatalog (NO GC)
    └── fixture_catalog.rs      # impl HistoryCatalog (NO GC)
```

**关键:** Mock 只实现 `HistoryCatalog`，自动获得 `HistoryCatalogGC` 的 `Unsupported` default impl

### Step 3: 效果对比

| 方案 | 修改点 | 代码量 | Review |
|------|------|------|--------|
| ❌ 当前 | 8 | 32 行 | 8 个 |
| ✅ CCP/CRP | 2 | 8 行 | 2 个 |
| **改进** | **-75%** | **-75%** | **-75%** |

### Step 4: 对标 SOLID 原则

| 原则 | 问题 | 解决 |
|------|------|------|
| **S** | Trait 两个改变原因 | 分成两个 trait |
| **O** | 新方法强制改 8 个 | 开放扩展（新 trait）、关闭修改 |
| **I** | 客户端被迫依赖 GC | 分离接口，只依赖需要的 |
| **L** | Mock 被迫 stub GC | Mock 无需实现 HistoryCatalogGC |

## 实现步骤

### Phase 1: 添加 GC Trait

1. 创建 `core/crates/layerfs-history/src/contract/catalog_gc.rs`
2. 定义 `HistoryCatalogGC: HistoryCatalog` trait（4 个方法）
3. 实现 default impl（返回 Unsupported）

### Phase 2: SQLite & PostgreSQL impl

1. 创建 `backend/sqlite/src/catalog_gc.rs`
   - 实现 4 个 GC 方法

2. 创建 `backend/postgresql/src/catalog_gc.rs`
   - 实现 4 个 GC 方法

### Phase 3: Mock（无需改动！）

```rust
impl HistoryCatalog for MockCatalog {
    // 只有 20+ 核心方法
    fn list_projects(...) -> Result<...> { /* ... */ }
    // ...
}

// 自动获得 HistoryCatalogGC 的 default impl（Unsupported）
// 如果测试需要 GC，使用真实 SQLite：
let catalog = Arc::new(SqliteCatalog::memory());
let gc_ops: Arc<dyn HistoryCatalogGC> = catalog.clone();
```

## 总结

**C9 不是「禁止新增方法」，而是「用组件原则指导设计」：**

- **CCP:** 分离因不同原因改变的代码
  - 核心查询 ← 业务逻辑变化
  - GC 操作 ← GC 实现迭代
  
- **CRP:** 按使用场景组织实现
  - 生产（SQLite + PostgreSQL）← 需要 GC
  - 测试（Mock）← 不需要 GC

**结果:** 8 个修改点 → 2 个修改点，新增 GC 方法成本从 Θ(n) 变为 O(1)
