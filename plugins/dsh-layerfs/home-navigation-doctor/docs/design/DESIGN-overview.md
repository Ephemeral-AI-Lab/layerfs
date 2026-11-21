# LayerFS Overview Page UI Design Specification v2 (`DESIGN-overview.md`)

> 初版设计记录。文中的纯中性单色是早期方案，当前总览使用语义色标记状态与页签。

> 本规范为 `dsh-layerfs` **Overview Page** 的从零重构版本（v2），面向「**去色彩噪点（极简纯中性单色）**、**一眼直观（开放式结构 + 清晰层级）**、**零盒中盒堆叠**」原则制定。所有样式与组件代码 100% 溯源于本文档。

---

## 1. 项目上下文

- **产品类型**：DeepSeek Harness (`dsh`) 桌面/Web 插件 —— **LayerFS Home Overview** 总览工作台。
- **目标视觉调性**：**极简瑞士单色工具美学（Monochrome Swiss Utility）**。摒弃多色标签、彩色图标堆砌与厚重卡片套叠，仅凭排版字重、留白间距、`1px` 极细分割线与单色明度层级建立直观秩序。
- **前端技术栈**：React 18 + TypeScript + CSS Modules (`*.module.css`) + `@deepseek-ai/dsh-client-ui-primitives` + `@deepseek-ai/dsh-client-ui-theme`。

---

## 2. 流派定位与构图策略

- **核心参考基准**：Apple macOS 极简信息列表 / Linear 单色工作台 / 瑞士国际主义平面网格。
- **开放式三段直观构图（零冗余外框）**：
  1. **开放式顶部页头（无外框、无阴影）**：
     - 左侧：`Overview` 主标题（`22px / 600 / -0.03em`）与轻量化运行时版本号（`LayerFS 2.0.0`，纯等宽弱文字，无胶囊边框）；第二行紧凑呈现当前健康摘要或首要待处理项，操作按钮靠右平齐。
     - 右侧：Presets 与 Refresh 两个 `30px × 30px` 极简图标按钮。
  2. **通栏底线资源切换栏（直观大字号计数 + 单色指示线）**：
     - 以一条贯穿全宽的 `1px` 底部分割线承载 `Projects`、`Stores`、`Machines` 三个大点击区原生 Tab；每个 Tab 直接并排显示名称与清晰等宽计数（`tabular-nums`），激活态仅以 `2px` 主墨色底边线标识（无彩色胶囊、无多余底色块）。
     - 右侧在同一基线上对齐主操作按钮（`New project` / `New machine`）。
  3. **轻量化筛选条 + 单色数据表**：
     - 搜索框与排序下拉统一 `32px` 高、两端对齐。
     - 数据表去色化处理：行内项目图标与 Store 图标统一降饱和为单色中性灰；`running` 状态改用克制单色细边框胶囊；表头保持极轻底色与统一数值右对齐。

---

## 3. 锁定态色彩系统 (Semantic Multi-Color 60 / 30 / 10)

保持开放式清爽排版与无多余卡片堆叠，同时启用克制、清晰的语义多色彩系统（对接 DSH 原生浅色/暗色主题令牌，严禁纯黑 `#000000` 与刺眼纯白 `#FFFFFF` 大面积画布）：

| 角色 | 占比 | 浅色模式 HEX | 暗色模式 HEX | CSS 变量映射 |
| --- | --- | --- | --- | --- |
| **主画布 Canvas** | 60% | `#F7F7F8` | `#121316` | `--ov-canvas` (`var(--dsw-alias-bg-base, #F7F7F8)`) |
| **主内容表面 Surface** | 30% | `#FCFCFD` | `#181A1F` | `--ov-surface` (`var(--dsw-alias-bg-layer-1, #FCFCFD)`) |
| **次级微弱底色 Surface Subtle** | 30% | `#F1F2F4` | `#1E2027` | `--ov-surface-subtle` (`var(--dsw-alias-bg-module-platform, #F1F2F4)`) |
| **品牌强调色 Accent (激活态/链接)** | 10% | `#2F5BB5` | `#4D6BFE` | `--ov-accent` (`var(--dsw-alias-state-business-primary, #2F5BB5)`) |
| **主文字 Ink Primary** | 10% | `#141517` | `#F2F3F5` | `--ov-ink-primary` (`var(--dsw-alias-label-primary, #141517)`) |
| **次文字 Ink Secondary** | — | `#5A5F6B` | `#9DA3AE` | `--ov-ink-secondary` (`var(--dsw-alias-label-secondary, #5A5F6B)`) |
| **弱文字 Ink Tertiary** | — | `#8A909E` | `#6E7480` | `--ov-ink-tertiary` (`var(--dsw-alias-label-tertiary, #8A909E)`) |
| **1px 标准边框 Border** | — | `rgba(24, 24, 27, 0.08)` | `rgba(255, 255, 255, 0.08)` | `--ov-border` (`var(--dsw-alias-border-l2, rgba(24, 24, 27, 0.08))`) |
| **语义色 · 运行中/正常 Success** | — | `#16A34A` | `#22C55E` | `--ov-success` (`var(--dsw-alias-state-success-primary, #16A34A)`) |
| **语义色 · 警告 Warning** | — | `#D97706` | `#F59E0B` | `--ov-warning` (`var(--dsw-alias-state-warn-primary, #D97706)`) |
| **语义色 · 错误 Danger** | — | `#DC2626` | `#F87171` | `--ov-danger` (`var(--dsw-alias-state-error-primary, #DC2626)`) |
| **交互悬停底色 Hover** | — | `rgba(24, 24, 27, 0.04)` | `rgba(255, 255, 255, 0.05)` | `--ov-hover` (`var(--dsw-alias-interactive-bg-hover, rgba(24, 24, 27, 0.04))`) |

---

## 4. 字体族与字阶表

- **Display / Body**：`var(--dsh-font-family-text-default, 'Inter', -apple-system, BlinkMacSystemFont, 'PingFang SC', 'Noto Sans SC', sans-serif)`
- **Mono**：`var(--ds-font-family-code, 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace)`

| 层级 | 字号 | 行高 | 字重 | 字间距 | 强制规则 |
| --- | --- | --- | --- | --- | --- |
| **Page Title (`h1.title`)** | `22px` | `28px` | `600` | `-0.03em` | 开放页头直接排版，无需外框 |
| **Tab Label** | `13.5px` | `20px` | `500` / `600` | `-0.01em` | 附带等宽数字 `tabular-nums` |
| **Status Headline** | `13px` | `20px` | `400` / `500` | `0` | `max-width: 65ch`，默认纯中性墨色 |
| **Table Cell / Row Name** | `13.5px` / `13px` | `20px` | `500` / `400` | `0` | 全量数值强制 `tabular-nums` |
| **Mono Meta / Version** | `12px` | `18px` | `400` | `0` | 等宽字体 + `tabular-nums` |

---

## 5. 间距栅格、按钮尺寸与表面规范

- **基础步长**：`4px` / `8px`；页面区块垂直间距 `16px`，表头与内容区分隔明确。
- **按钮尺寸统一**：
  - **导航栏 (`NavBar`)**：统一 `28px` 高（图标按钮 `28×28`、`Jump to…` / `Add` 均为 `28px`）。
  - **页头工具与状态操作**：图标按钮 `30×30`，状态行动作按钮 `28px` 高。
  - **标签工具栏与筛选栏**：主动作按钮、搜索框、`Clear search`、排序下拉统一 `32px` 高，`border-radius: 6px`。
- **表面与阴影**：
  - 顶部页头与标签栏完全扁平化（仅用 `1px` 底部分割线建立秩序）。
  - 数据表与折叠告警列表使用 `1px solid var(--ov-border)` + `border-radius: 8px` + 极克制多层环境光阴影：
    - 浅色：`0 1px 2px rgba(24, 24, 27, 0.03), 0 4px 12px -4px rgba(24, 24, 27, 0.04)`
    - 暗色：`inset 0 1px 0 rgba(255, 255, 255, 0.05), 0 6px 16px -6px rgba(0, 0, 0, 0.35)`

---

## 6. 交互状态与动效预算

- **Hover**：仅限中性灰微调（`--ov-hover`）或边框提亮（`--ov-border-hover`），严禁 `hover:scale`。
- **Focus Visible**：`2px solid var(--ov-accent)`，`outline-offset: 2px`。
- **动效曲线**：`140ms cubic-bezier(0.16, 1, 0.3, 1)`，完整支持 `prefers-reduced-motion: reduce`。

---

## 7. 全局界面精简与防平庸校验声明

- [x] **零文字前圆点**：全局 `StateChip`、Overview 状态行、`running` 胶囊、会话/Sandbox 状态、Store/Bucket/PG 卡片与 Tab 均不在静态文字前放置圆点（仅运行中加载态保留旋转指示器）。
- [x] **去小字介绍与无用信息**：移除 Overview 运行时原始 commit hash、空态教程长句（`ovNoProjectsBody`、`noStoresBody`、`ovNoBucketsBody`）、Doctor 页尾冗余 CLI 来源说明与无意义占位副标题。
- [x] **全局按钮文字严格对齐**：全站按钮与下拉框统一 `box-sizing: border-box`、`inline-flex` 垂直居中、一致 `line-height`，并强制按钮内 `svg { display: block; flex: none; }` 消除基线偏移。
- [x] **无紫蓝渐变 / 无发光球 / 无彩色圆底图标 / 无 `hover:scale`**。
- [x] **无纯黑 `#000000` 与刺眼纯白 `#FFFFFF` 主画布**，全量数字开启 `tabular-nums`，正文限宽 `max-width: 65ch`。
- [x] **100% 溯源本设计文档**。
