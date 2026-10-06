> Status: Research; informative and not a product contract.

# LayerFS extent normalization 与 retained slices 分析

- 日期：2026-10-02
- 检查的源码提交：`ffdfa022f21930f3e7325b95e6ae5c2b11b7d2a0`
- 范围：当前 `core/` 的文件局部编辑、extent coalescing、映射树拼接与读取路径。
- 性质：源码分析与已有测试证据解读；不是性能测量或发布资格证明。

## 结论

当前 extent normalization 不会再次 rechunk retained slices。它只合并对同一 payload 的连续引用，以及调整 extent tree 的映射节点；保留的 payload 不会因为 normalization 重新进入 FastCDC。

这一实现保护了局部更新和旧 chunk 复用，但没有同时解决内容碎片整理、由最终文件字节决定的唯一表示、存储回收等问题。本次确认了一处注释与测试名称的表述问题，识别了若干设计代价，并保留一个尚未复现的跨叶页归一化验证点。没有发现已确认的数据正确性缺陷。

## 1. 为什么 normalization 不会 rechunk

### 1.1 Slice 合并只操作引用描述

实际规则位于 [`coalesce_adjacent`](../../core/crates/layerfs-content/src/file/edit/concat.rs)。两个 slice 必须同时满足：

```text
previous.payload_id == next.payload_id

previous.source_offset + previous.length
    == next.source_offset
```

合并后仍使用原来的 payload ID 和起始 source offset，长度取二者之和。例如：

```text
(P, offset=0,   length=100)
(P, offset=100, length=200)
                ↓
(P, offset=0,   length=300)
```

该函数没有 payload reader，没有字节拼接，也没有 CDC 调用。它只处理 ID、offset 和 length。

### 1.2 树的重新分区只重建 metadata

[`tree::coalesce`、`concat_inner` 和 `root_from_extents`](../../core/crates/layerfs-content/src/file/edit/tree.rs) 操作 extent 条目和 child summaries。叶节点拼接中的核心步骤是：

```rust
extents.extend(other);
coalesce(&mut extents)?;
root_from_extents(objects, extents)
```

可能变化的是 extent 条目数量、条目在映射页中的分布、映射页 ObjectId 和文件 root。这里重新编码和哈希的是映射 metadata，保留的 chunk payload 没有被重新分块。

因此，必须区分两个概念：

- **Extent 边界**：映射中的引用描述边界，冗余的连续引用可以合并。
- **Payload 分块**：底层 chunk 对象及其字节内容，不会因为上述 normalization 改变。

### 1.3 局部编辑只对 replacement 运行 CDC

[`replace_chunked`](../../core/crates/layerfs-content/src/file/edit/apply.rs) 拆出左侧、被移除部分和右侧，只将 replacement reader 交给 FastCDC，再拼接三部分。纯删除没有 replacement scan。切点落在旧 chunk 内部时，`split` 创建沿用旧 payload ID 的 slices。

该结论适用于 chunked → chunked 的已知范围局部编辑。完整文件重新构造，以及 whole-file → chunked 的表示转换，属于不同路径；后者的 `stream_combined` 会把完整结果交给 streaming builder。不能将“不 rechunk retained slices”推广为所有构造路径的保证。

## 2. 已确认的表述问题：文件相邻不等于源区间连续

[`concat.rs` 的模块注释](../../core/crates/layerfs-content/src/file/edit/concat.rs) 说，删除 chunk 内的一段会留下需要合并的两个 slices。对于非空的中间删除，这个描述不成立。

例如删除 payload P 中的 `[100, 200)`，保留部分为：

```text
P[0:100] + P[200:end]
```

它们在新文件中相邻，但源区间有缺口：`100 != 200`。当前实现正确地拒绝合并。如果强行合并为一个连续源区间，就会把已删除的字节重新带回来。

[`edit_model.rs`](../../core/crates/layerfs-content/tests/edit_model.rs) 中的 `a_deletion_that_merges_two_slices_stays_canonical` 也有同样的问题：名字和注释声称发生合并，但测试实际通过独立字节模型检查结果，没有断言 coalescing 发生或 extent 数量减少。

该测试通过只能支持它实际执行的断言，不能证明名称声称的合并行为。这是证据表述问题；本次没有发现实现错误合并并导致数据损坏。

## 3. 不 rechunk 的设计代价：小 slice 与映射碎片

当前 coalescing 不会合并以下情况：

- 不同 payload 的小 slices。
- 同一 payload 内存在源偏移缺口的 slices。
- 新插入的小 payload 与旁边的旧 payload。

多次小编辑后，原来的大 extent 可以变成多个小 extents。FastCDC 的 8 KiB 最小切块参数不能约束 retained slices：[`ExtentSlice::new`](../../core/crates/layerfs-content/src/file/mapping/types.rs) 接受非零长度的 slice，1 字节 slice 也合法。

由此可能增加：

- 映射条目和映射页数量。
- 映射遍历与引用处理开销。
- 读取很小的 slice 时的 payload 获取与解码放大。

[`读取路径`](../../core/crates/layerfs-content/src/file/mapping/read.rs) 先取得并解码所引用的 chunk，再取对应的 slice。同一 read wave 内按 payload ID 去重，因此不能简单按“每个 slice 都独立读取一次完整 chunk”计算实际成本。

[`MAXIMUM_EDITS_PER_OPERATION = 4096`](../../core/crates/layerfs-content/src/file/edit/input.rs) 限制的是一次调用的编辑数量。它不能证明经过多轮保存后，extent 密度始终接近初次 CDC 构造时的水平。

以上是机制上成立的代价。本次没有测量其在真实工作负载中的严重程度，也不把容量常量当成性能或内存证据。

## 4. Normalization 没有建立“同样字节只有一个 root”

局部编辑保留旧分块，只扫描 replacement；完整构造则对整个文件运行 CDC。两条路径即使读回字节相同，也不必得到相同的 chunk partition 和文件 root。

已有源码证据：

- [`edit_transitions.rs`](../../core/crates/layerfs-content/tests/edit_transitions.rs) 明确说明，chunked 局部编辑不与 fresh construction 比较 root，因为二者使用不同的分块路径。
- [`EditStream::new`](../../core/crates/layerfs-content/src/file/edit/input.rs) 保留相邻编辑的独立分段，明确指出合并相邻编辑会改变 chunk boundaries 和最终 root。

因此，当前 normalization 建立的是合法的 extent 表示、部分冗余引用的消除和既定树分区规则。它没有建立仅由最终文件字节决定的唯一表示。

实际含义是：

- 相同 root 可以表示完全相同的内容表示。
- 不同 root 不能直接推出文件字节不同。
- 相同字节通过不同构造或编辑路径生成，不保证整文件 root 级的精确去重。

这属于身份语义和去重范围的限制，不能仅凭它判定实现有 bug。

## 5. 待验证点：跨叶页的可合并 slice 对

[`ExtentNode::validate`](../../core/crates/layerfs-content/src/file/mapping/types.rs) 检查同一叶页内部的相邻 slices。等高 branch 的 [`concat_inner`](../../core/crates/layerfs-content/src/file/edit/tree.rs) 拼接 child summaries，没有在该分支直接检查左右子树交界处的 payload slices。

因此，页内 validation 通过还不足以单独证明跨叶页不存在可合并的 slice 对。其他 split/concat 步骤可能已经处理了交界，需要定向验证。

建议构造以下案例：同一个 payload 的两个源区间连续 slices，恰好位于两个叶页的交界处，再执行能够触发相关 branch join 的操作。检查最终映射、读回字节和应当遵循的参考分区规则。

**本次没有复现这一问题，不能把它报告为已确认的 bug。** 即使存在漏合并，它首先影响结构归一化与表示身份，也不意味着 retained payload 被 rechunk。

## 6. 建议的后续工作

1. 修正“删除 chunk 中间一段会产生可合并 slices”的注释，以及没有断言实际合并的测试名称。
2. 定向覆盖同 payload 连续源区间可合并、源区间有缺口不可合并、不同 payload 不可合并的规则。
3. 补充跨叶页交界处的 coalescing 验证，并将应保持的参考分区行为写入断言。
4. 对多轮小编辑后的 extent 密度、实际对象获取／解码开销和存储占用进行独立评估；如执行测量，遵循仓库的一次取样、cache、身份与预算规则。
5. 如果以后需要内容级碎片整理，应明确设计为可能重写 payload、改变 root 的操作，不能隐式放入现有 normalization。

## 7. 证据与验证范围

本轮检查了生产源码和测试断言，没有修改产品实现，也没有重复运行前一轮已通过的测试。没有新增性能测量，没有执行全 workspace 检查。

本对话前一轮在上述提交上执行的 core 定向测试结果为：

| 测试目标 | 结果 | 实际证明范围 |
| --- | --- | --- |
| [`edit_model`](../../core/crates/layerfs-content/tests/edit_model.rs) | 6/6 通过 | 包含 insert/delete/overwrite 的结果字节模型等价 |
| [`edit_localized`](../../core/crates/layerfs-content/tests/edit_localized.rs) | 8/8 通过 | 定向的 payload 身份复用、局部访问、append 与删除约束 |
| [`edit_reference`](../../core/crates/layerfs-content/tests/edit_reference.rs) | 3/3 通过 | 包含九组案例的 root、映射页分区和保留叶节点身份参考比较 |

这些测试结果不建立任意编辑历史下的碎片成本上限，也不单独关闭上述跨叶页验证点。测试输出位于本对话的执行记录；本文件没有新增或重写历史测试 receipt。

前一轮的 core 复现命令，在仓库根目录运行：

```sh
CARGO_TARGET_DIR="$PWD/core/target" LAYERFS_CONSTRUCTION_WORKERS=1 \
cargo +1.85.1 test --manifest-path core/Cargo.toml --locked \
-p layerfs-content --test edit_model --test edit_localized \
--test edit_reference -- --nocapture
```

此命令是正确性测试命令，不是性能测量命令。
