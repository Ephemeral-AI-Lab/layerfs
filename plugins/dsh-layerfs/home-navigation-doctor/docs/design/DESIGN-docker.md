# LayerFS Docker Machine UI Design Specification (`DESIGN-docker.md`)

> 初版设计记录。文中的尺寸、交互和视觉约束用于记录当时的设计目标，当前行为见实现及用户指南。

> 本文档为 `dsh-layerfs` **Docker 机器界面（`MachineDetail` / `Overview` / `Spec` / `Readiness` / `MachineDialogs`）** 重构的锁定态设计规范，严格落实「**窗口/容器尺寸一致**、**短文字按钮在前、长文字按钮在后**、**元素分区整洁无冗余**」。所有样式与布局改动 100% 溯源于本文档。

---

## 1. 项目上下文

- **产品类型**：LayerFS Center 中 Docker 机器详情页及其弹窗（`docker` 机器页、Docker 阻塞/就绪修复面板、Docker Socket 设置与强制停止/移除弹窗）。
- **目标视觉调性**：克制精密的开发者工具面板（Swiss Utility Grid + DSH 原生语义多色彩），确保页面各标签页宽度、卡片内边距、圆角以及所有弹窗窗口尺寸完全一致，交互按钮按“短文案在前、长文案在后”清晰排序。
- **前端技术栈**：React 18 + TypeScript + CSS Modules (`machines.module.css`, `log.module.css`, `ui.module.css`) + `@deepseek-ai/dsh-client-ui-primitives`。

---

## 2. 流派定位与构图策略

- **统一容器与窗口尺寸**：
  - **页面级容器**：Docker 机器页及所有子标签页（Overview / Sandboxes / Stores / Images / Files / Worker log）统一使用 `<Page wide>` 标准宽度与 `16px` 纵向卡片间距，头部与标签栏结构与其他机器页保持一致的视觉对齐基准。
  - **卡片级容器**：Usage 卡、Details 卡、Fallback 事实卡、Notice 告警条统一 `width: 100%`、`box-sizing: border-box`、`border-radius: 10px`、`1px` 细边框与 `14px 16px` 统一内边距。
  - **弹窗级窗口尺寸**：Docker 相关弹窗（`SettingsDialog`、`ForceStopDialog`、`RemoveDialog`）以及 `SpaceDialog` / `NewSandboxDialog` 统一使用标准宽度 `width: min(520px, calc(100vw - 32px))`，消除弹窗忽宽忽窄的跳动感。
- **按钮排序铁律（短文字在前，长文字在后）**：
  - **Details 卡片头操作栏**：短按钮在前、长按钮在后（如 `Run diagnostics` / `运行诊断` 在前，`Update settings…` / `更新设置…` 在后；并在所有按钮组容器统一 `display: inline-flex; align-items: center; gap: 8px;`）。
  - **Docker 就绪修复面板 (`FixPanel`)**：将原本分散在不同段落的操作控件收拢至底部统一操作行 `.fixActions`，严格按短文案到长文案顺序排列：`Check again`（短） → `Fix in setup`（中） → `Choose another socket`（长）。
  - **弹窗底部按钮 (`Modal footer`)**：`Save` / `保存` 或 `Cancel` / `取消` 等短文案按钮置前，长文案确认按钮（如 `Force stop` / `Remove everything`）置后，所有按钮统一 `height: 30px`、`line-height: 1`、`inline-flex` 居中。
- **元素整理与去噪**：
  - 移除 `MachineDetail` 页头重复的小字 `intro`（`Docker · Containers` 已在 Details 卡片中清晰展示）。
  - 整理 Docker 事实行（`.factRow[data-docker]`）为规则等宽卡片网格（`repeat(auto-fit, minmax(180px, 1fr))`），与 `UsageCard` 和 `MachineDetailsCard` 共享相同边框与内边距，不再散落为无边框孤立列表。

---

## 3. 锁定态色彩系统 (60 / 30 / 10)

严禁纯黑 `#000000` 与刺眼纯白 `#FFFFFF` 大面积画布，映射 DSH 主题语义令牌支持浅色/暗色自适应：

| 角色 | 占比 | 浅色模式 HEX | 暗色模式 HEX | CSS 变量映射 |
| --- | --- | --- | --- | --- |
| **主画布 Canvas** | 60% | `#F7F7F8` | `#121316` | `var(--dsw-alias-bg-base, #F7F7F8)` |
| **卡片表面 Surface** | 30% | `#FCFCFD` | `#181A1F` | `var(--dsw-alias-settings-card-fill, var(--dsw-alias-bg-layer-1, #FCFCFD))` |
| **微弱抬升/条带 Surface Subtle** | 30% | `#F1F2F4` | `#1E2027` | `var(--dsw-alias-bg-module-platform, #F1F2F4)` |
| **强调色 Accent** | 10% | `#2F5BB5` | `#4D6BFE` | `var(--dsw-alias-state-business-primary, #2F5BB5)` |
| **主文字 Ink Primary** | 10% | `#141517` | `#F2F3F5` | `var(--dsw-alias-label-primary, #141517)` |
| **次文字 Ink Secondary** | — | `#5A5F6B` | `#9DA3AE` | `var(--dsw-alias-label-secondary, #5A5F6B)` |
| **弱文字 Ink Tertiary** | — | `#8A909E` | `#6E7480` | `var(--dsw-alias-label-tertiary, #8A909E)` |
| **1px 标准边框 Border** | — | `rgba(24, 24, 27, 0.09)` | `rgba(255, 255, 255, 0.08)` | `var(--dsw-alias-border-l2, rgba(24, 24, 27, 0.09))` |
| **语义色 Success / Warning / Danger** | — | `#16A34A` / `#D97706` / `#DC2626` | `#22C55E` / `#F59E0B` / `#F87171` | `--dsw-alias-state-success/warn/error-primary` |

---

## 4. 字体族与字阶表

- **Display / Body**：`var(--dsh-font-family-text-default, 'Inter', -apple-system, BlinkMacSystemFont, 'PingFang SC', 'Noto Sans SC', sans-serif)`
- **Mono**：`var(--ds-font-family-code, 'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace)`
- **字阶**：
  - Page Title：`20px / 28px / 600 / -0.02em`
  - Card Title：`13.5px / 20px / 600`
  - Fact Value / Body：`13px / 20px / 400`，说明段落限制 `max-width: 65ch`
  - Fact Label / Meta：`12px / 16px / 500`，全量数字/容量/时间强制 `font-variant-numeric: tabular-nums`

---

## 5. 间距栅格与表面规范

- **基础步长**：`4px` / `8px`；页面卡片栈 `.ovStack` 统一 `gap: 16px`。
- **统一弹窗宽度**：`.dialog` 统一 `width: min(520px, calc(100vw - 32px))`。
- **统一按钮尺寸与对齐**：所有操作按钮统一 `height: 30px; padding: 0 12px; display: inline-flex; align-items: center; justify-content: center; line-height: 1; white-space: nowrap;`，图标按钮正方 `30px × 30px`。
- **边框与多层环境光阴影**：
  - 统一 `1px solid var(--dsw-alias-border-l2)`。
  - 浅色：`0 1px 2px rgba(24, 24, 27, 0.04), 0 6px 18px -4px rgba(24, 24, 27, 0.05)`；暗色：`inset 0 1px 0 rgba(255, 255, 255, 0.06), 0 8px 24px -6px rgba(0, 0, 0, 0.42)`。

---

## 6. 交互状态与动效预算

- **Hover**：微弱底色或边框提亮（`140ms cubic-bezier(0.16, 1, 0.3, 1)`），严禁 `hover:scale`。
- **Focus Visible**：`2px solid var(--dsw-focus-ring-color, var(--dsw-alias-link))`，`outline-offset: 2px`。
- **Reduced Motion**：完整尊重 `@media (prefers-reduced-motion: reduce)`。

---

## 7. 防平庸校验声明

- [x] 无紫蓝渐变、无模糊光晕球、无彩色圆底小图标、无 `hover:scale`。
- [x] 无文字前静态圆点，无重复小字介绍。
- [x] 窗口与容器尺寸 100% 统一，按钮严格遵循「短文字在前、长文字在后」。
- [x] 数字与容量强制 `tabular-nums`，正文限制 `max-width: 65ch`，100% 溯源本文档。
