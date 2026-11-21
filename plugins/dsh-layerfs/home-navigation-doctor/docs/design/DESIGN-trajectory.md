# DESIGN-trajectory — LayerFS 2.0 轨迹时间线（Trajectory Timeline）界面规范

> 初版设计记录。当前轨迹界面的行为见实现及用户指南。

## 1. 项目上下文
- **产品类型**：LayerFS 2.0 会话轨迹与时间线分析面板（对应 `TrajectoryView.tsx`、`TrajectoryTimeline.tsx`、`trajectory.module.css`）
- **目标视觉调性**：高密度桌面开发观测工具、瑞士国际主义时间轴栅格、语义多色彩泳道与清晰交互层级
- **前端技术栈**：React 18 + TypeScript + CSS Modules + `@deepseek-ai/dsh-client-ui-primitives` + `@deepseek-ai/dsh-client-ui-theme`

## 2. 流派定位与构图策略
- **核心基准**：专业性能分析器（Profiler / Trace Timeline）四泳道对齐构图
- **卡片内部结构（`.controlsCard`）**：
  - **顶部工具栏（`.toolbar`）**：左侧为等高（`30px`）图标+文字筛选开关组（`轮次`、`LayerFS 调用`、`只看 LayerFS 写入`），右侧为等高（`30px`）紧凑搜索框，左右对齐 `16px` 边距，底部分割线 `0.5px`。
  - **时间线标题栏（`.timelineHead`）**：左侧为 `14px` 粗体标题，右侧为 `12.5px` 等宽数字汇总（`tabular-nums`），基线居中对齐。
  - **四泳道时间轴（`.timeline`）**：左侧 `80px` 标签列右对齐（距轴线 `12px`），右侧弹性泳道区包含顶部刻度轴（`0 / 50% / 100%`）与四条等高（`28px`）泳道（`输入 / 模型 / 工具 / LayerFS`），底部留白与顶部留白对称平衡（`14px`）。

## 3. 锁定态色彩系统 (60/30/10)
延续系统主题与语义多色彩体系，严禁纯黑 `#000000` 与纯白 `#FFFFFF` 大面积铺底：

| 层级 | 语义用途 | 主题 Token |
| --- | --- | --- |
| 60% 主画布 | 页面背景 | `var(--dsw-alias-bg-layer-1)` |
| 30% 次级表面 | 控制与时间线卡片 | `var(--dsw-alias-settings-card-fill)` |
| 10% 语义泳道 | 输入泳道（Input） | `var(--dsw-alias-state-business-primary)` |
| 10% 语义泳道 | 模型泳道（Model） | `var(--traj-model)` |
| 10% 语义泳道 | 工具泳道（Tools） | `var(--dsw-alias-state-warn-label)` |
| 10% 语义泳道 | LayerFS 写入 / 拒绝 / 普通 | `state-business-primary` / `state-error-primary` / `label-tertiary` |
| 边框与网格 | 1px / 0.5px 极细分割线 | `var(--dsw-alias-border-l1)` / `var(--dsw-alias-border-l2)` / `var(--dsw-alias-settings-card-stroke)` |

## 4. 字体族与字阶表
- **Display / Body**：系统无衬线字体栈（继承 `--ds-font-family-base`）
- **Mono（命令、工具名、版本号、刻度数字）**：`var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace)`
- **字阶规范**：
  - 时间线标题（`.timelineHead h2`）：`14px` / 行高 `20px` / 字重 `600` / `letter-spacing: -0.01em`
  - 工具栏按钮与搜索框（`.action`、`.searchInput`）：`13px` / `line-height: 1`
  - 统计摘要（`.counts`）与泳道标签（`.laneLabel`）：`12.5px` / `tabular-nums`
  - 时间轴刻度（`.axis`）：`11.5px` / `tabular-nums` / 次级文字色
  - 辅助说明（`.note`）：`max-width: 65ch` / 行高 `1.5`

## 5. 间距栅格与表面规范
- **基础步长**：`4px` / `8px`
- **工具栏统一尺寸**：
  - `.toolbar`：`padding: 10px 16px; min-height: 50px; gap: 8px 16px;`
  - `.action` 与 `.search` 统一 **`height: 30px`**、`border-radius: var(--dsw-radius-md)`
  - 折叠/展开图标替换粗糙 Unicode 字符为统一 `14x14` 线性 SVG 图标，所有图标设 `display: block; flex: none;`，与文字 `gap: 6px` 严格垂直居中。
  - `.action[aria-checked='true']` 使用轻度品牌色描边与柔和填充，明确区分开启状态。
- **时间轴对称留白与清晰度**：
  - `.timelineHead`：`padding: 14px 16px 10px;`
  - `.timeline`：`grid-template-columns: 80px minmax(0, 1fr); padding-bottom: 14px;`
  - `.axis`：`padding: 0 16px 6px 0;` 与 `.lanes` 左右完全对齐
  - `.laneLabels`：`padding: 0 12px 0 16px; border-right: 0.5px solid var(--dsw-alias-border-l2);`
  - `.lanes`：`padding: 0 16px 0 0;`
  - 泳道高度统一 `28px`，时间条（`.bar`）高 `14px`、圆角 `4px`、垂直居中（`top: 7px`），最小宽度 `5px`；事件刻度（`.tick`）宽 `5px`、圆角 `3px`。

## 6. 交互状态与动效预算
- **过渡时长与曲线**：`150ms cubic-bezier(0.16, 1, 0.3, 1)`，用于按钮背景、边框高亮与时间条悬停反馈。
- **悬停反馈**：时间条悬停提亮并显示细轮廓环，泳道悬停微弱提亮，禁止 `hover:scale`。

## 7. 防平庸校验声明
- 无紫蓝渐变、无背景光晕球、无滥用彩色圆底图标、无占位假文案。
- 无纯黑 `#000000` / 纯白 `#FFFFFF` 主画布。
- 所有时间刻度与计数已应用 `tabular-nums`，说明文本受 `max-width: 65ch` 限制。
- 本规范文档已落盘，后续 `TrajectoryView.tsx` 与 `trajectory.module.css` 改动 100% 溯源本规范。
