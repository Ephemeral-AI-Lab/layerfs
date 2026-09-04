# LayerFS v0.1.2 故事版 X 回复串

在中文版 X Article 发布帖下发送以下三条回复。复制代码块正文，并附上标注图片。

## 回复 1/3——谜题

```text
1/3——想象在一本巨厚的书前面加一页。

慢办法会重写后面的每一张旧页。

LayerFS 保留旧页，只修改一张很小的目录卡片：

新页面 | 指向同一本旧书的引用

这就是 v0.1.2 的核心想法。正在运行的 Workspace 使用一张由新旧片段组成的草稿清单；Commit 再把最终清单保存进已有 extent tree。

extent tree 早于 v0.1.0。v0.1.2 新增的是从实时编辑到引用式保存结构的桥梁。

一套方法覆盖 prepend、append、overwrite、insert、delete、grow、shrink、truncate 和 zero-extension：

旧前缀 + 替换内容 + 旧后缀

改变指针，保留未变化字节。
```

附图：

- `./images/01-edit-locality-zh-CN.png`
- `./images/02-workspace-edit-pipeline-zh-CN.png`

## 回复 2/3——算法与证据

```text
2/3——Big-O 故事很简单。

令 N 为旧文件大小，a 为新字节数量。

复制式编辑：Θ(N + a)
LayerFS 草稿编辑：O(a + 树高 + 被删除片段)
完整读取或哈希：仍为 Θ(N)

LayerFS 不会让所有操作变成 O(1)。它删除的是范围已知编辑中的强制整文件重写。

然后我们测试真实代码是否符合这个想法。

一次 4 KiB 头部插入加 Commit：

• 1 MiB 文件：4.680 ms
• 10 MiB：4.883 ms
• 100 MiB：7.257 ms
• 500 MiB：14.300 ms

我们没有停在一个幸运用例：56 种编辑、560 次计时运行、112 条独立正确性证明，覆盖 1/10/100/500 MiB 文件。
```

附图：

- `./images/06-big-o-table-zh-CN.png`
- `./images/03-prepend-scaling-zh-CN.png`
- `./images/05-evidence-matrix-zh-CN.png`

## 回复 3/3——对比与边界

```text
3/3——我们还测试了固定 Cloudflare Computer 的真实 FUSE 路径。

在 100 MiB 文件上执行三次 4 KiB 编辑：

• 覆盖：LayerFS 6.928 ms / Cloudflare 路径 225.8 ms——33×
• 中部插入：7.752 ms / 3,040.9 ms——392×
• 头部插入：7.257 ms / 5,827.6 ms——803×

为什么？已测 Cloudflare 路径构建缓冲文件状态，并为位置编辑移动字节。LayerFS 修改引用，只发布新字节和变化树节点。

API 与计时边界不同，因此这不是“产品普遍快 803×”的声明。它比较固定源码下的完整已测路径。Cloudflare 活动完成了 168 次计时运行和 168 次独立字节正确性检查。

LayerFS v0.1.2 仍是仅源代码 Developer Preview，不承诺崩溃或断电持久性。

https://github.com/Ephemeral-AI-Lab/layerfs/releases/tag/v0.1.2
```

附图：

- `./images/04-cloudflare-comparison-zh-CN.png`
