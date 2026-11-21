# DESIGN-doctor — LayerFS 2.0 诊断界面（Doctor Page）排版与对齐规范

> 初版设计记录。当前诊断界面的行为见实现及用户指南。

## 1. 项目上下文
- **产品类型**：LayerFS 2.0 桌面端诊断与健康检查界面（Doctor Page，对应 `DoctorPage.tsx`、`Row.tsx`、`DoctorPage.module.css`、`tips.tsx`）
- **目标视觉调性**：高密度桌面工程工具、瑞士国际主义列表栅格、语义多色彩状态提示、零冗余装饰与零文字错位
- **前端技术栈**：React 18 + TypeScript + CSS Modules + `@deepseek-ai/dsh-client-ui-primitives` + `@deepseek-ai/dsh-client-ui-theme`

## 2. 流派定位与构图策略
- **核心基准**：瑞士国际主义等列对齐网格（Swiss Typographic Grid）
- **页面结构**：
  - **页头区（`.headRow`）**：左侧为状态标题与数字统计摘要（左边界严格对齐），右侧为全局主操作按钮（`Check all` / 全部检查），垂直居中对齐。
  - **顶层安装卡片（`.homeBox`）与分类列表卡片（`.card`）**：共享完全一致的圆角、边框与水平内边距，保证树状每一行的「名称列 / 状态列 / 说明与事实列 / 检查标记列 / 操作按钮列」自上而下形成贯穿整页的对齐基线。
  - **展开详情面板（`.detail`）**：使用统一标签列宽 `--doc-lbl-w: 44px`，命令行、检查项清单左边缘严格齐平；检查项清单（`.checks li`）采用双列栅格，消除不同长度检查名导致的右侧结果文本参差不齐。

## 3. 锁定态色彩系统 (60/30/10)
延续系统主题与语义多色彩体系，严禁纯黑 `#000000` 与纯白 `#FFFFFF` 大面积铺底：

| 层级 | 语义用途 | 浅色主题 Token / HEX | 暗色主题 Token / HEX |
| --- | --- | --- | --- |
| 60% 主画布 | 页面背景 | `var(--dsw-alias-bg-layer-1)` (`#F7F6F2` / 主题层) | `var(--dsw-alias-bg-layer-1)` (`#111214` / 主题层) |
| 30% 次级表面 | 卡片 / 展开面板 | `var(--dsw-alias-settings-card-fill)` / `var(--dsw-alias-bg-module-platform)` | `var(--dsw-alias-settings-card-fill)` / `var(--dsw-alias-bg-module-platform)` |
| 10% 强调与语义 | 正常 / 成功 (`--doc-ok`) | `#15803D` | `#22C55E` |
| 10% 强调与语义 | 错误 / 问题 (`--doc-bad`) | `#D41414` | `#F76A6A` |
| 10% 强调与语义 | 警告 / 提示 (`--doc-warn`) | `#B45309` | `#F5A524` |
| 10% 强调与语义 | 运行中 / 链接 (`--doc-run` / `--doc-link`) | `#3767D6` / `#2F5FCF` | `#7AAAFF` / `#8FB8FF` |
| 10% 强调与语义 | 未检查 / 中性 (`--doc-hollow`) | `#73787F` | `#ADB2B8` |
| 边框 | 1px / 0.5px 低透明分割线 | `var(--dsw-alias-border-l2)` / `var(--dsw-alias-settings-card-stroke)` | `var(--dsw-alias-border-l2)` / `var(--dsw-alias-settings-card-stroke)` |

## 4. 字体族与字阶表
- **Display / Body**：系统无衬线字体栈（继承 `--ds-font-family-base` / `Inter`, `PingFang SC`, `Noto Sans SC`）
- **Mono（命令、检查名、摘要哈希）**：`var(--ds-font-family-code, ui-monospace, SFMono-Regular, Menlo, monospace)`
- **字阶与对齐规则**：
  - 页面状态标题（`.title` / `.headSig`）：`20px` / 行高 `28px` / 字重 `500` / `letter-spacing: -0.015em` / `tabular-nums`
  - 列表行正文（`.name`、`.sigWord`、`.word`、`.fact`）：`13px` / 行高 `20px`
  - 详情标签、检查结果、提示卡片（`.lbl`、`.res`、`.tip`）：`12px` / 行高 `18px`
  - 命令行与检查名代码（`code`）：`12px` / 行高 `18px`
  - 按钮文字（`.btn`）：`13px` / `line-height: 1` / `display: inline-flex; align-items: center; justify-content: center;`
  - 计数、耗时、统计数字：强制 `font-variant-numeric: tabular-nums`
  - 说明段落与确认弹窗正文（`.intro`、`.modalBody`、`.res`）：限制 `max-width: 65ch`

## 5. 间距栅格与表面规范
- **基础步长**：`4px` / `8px`
  - **容器圆角与上下对称间距**：
    - 页面主容器（`.root`）统一 `gap: 24px`；页头区（`.head`）设 `padding-top: 24px; gap: 20px;`
    - `.homeBox` 与 `.card` 统一 `border-radius: var(--dsw-radius-xl)`、`padding: 8px 0`；`.group` 设 `padding: 6px 0` 且 `:first-child { padding-top: 0; }`、`:last-child { padding-bottom: 0; }`，保证所有卡片首尾上下留白严格对称（均为 `8px`），分组之间为对称 `12px`
    - 列表行（`.line`）拉大至 `min-height: 48px`、`gap: 4px 12px`、`margin: 0 6px`、`padding: 6px 10px 6px 4px`（子行 `min-height: 42px; padding-block: 4px;`）
    - 展开详情（`.detail`）采用上下对称外边距 `margin: 8px 14px 8px calc(38px + var(--depth) * 24px)`、`padding: 14px 16px`、`gap: 12px`，检查项列表（`.checks`）行距拉大至 `gap: 8px`
  - **页头对齐**：`.headRow` 垂直居中且右侧保留 `16px` 留白（与 `.line` 的 `6px + 10px` 右偏移一致），使 `Check all` 右边界与下方每一行 `.acts` 中的 `Check` 按钮右边界垂直对齐；`.headSig` 移除左内边距（`padding: 0`），保证状态标题与下方 `.intro` 左缘对齐。
  - **按钮尺寸与对齐**：页面内所有操作按钮（`.btn`、`.more .btn`、`data-w="all"`）统一为 **`height: 28px`**、`padding: 0 12px`、`font-size: 12.5px`、`line-height: 1`；短按钮（`check` / `copy` / `Stop` / `Fix`）统一 `min-width: 68px`，长按钮（`full` / `all`）统一 `min-width: 88px`，`.acts` 列宽固定 `min-width: 164px`（`88 + 8 + 68`）确保整页所有操作按钮尺寸与右缘严格成列；弹窗底部按钮（`.modalActions button`）统一 `height: 30px; min-width: 72px;`。
  - **行内垂直居中**：`.line`、`button.rowBtn`、`.sig`、`.more`、`.cause`、`.inside`、`.acts` 全部统一 `align-items: center`，内部 SVG 均设 `display: block; flex: none;`。
  - **展开详情列对齐**：`.detail` 定义 `--doc-lbl-w: 44px`；`.cmdLine` 顶部与单行居中自然对齐（`.lbl` 宽 `44px` 右对齐、`line-height: 18px`、`padding-top: 6px`，`code` 设 `padding: 5px 10px; line-height: 18px; min-height: 28px; box-sizing: border-box;`）；`.checks` 左缩进 `calc(var(--doc-lbl-w) + 8px)`；`.checks li` 采用二列网格 `minmax(148px, 196px) minmax(0, 1fr)` 且 `align-items: center`，保证所有检查项与结果文字严格成列。
  - **悬浮提示卡对齐**：`.tip li` 与 `.sigCard li` 采用 `align-items: center; line-height: 18px;`，图标、检查名与状态文案水平中线对齐。

## 6. 交互状态与动效预算
- **过渡时长与曲线**：`150ms cubic-bezier(0.16, 1, 0.3, 1)`，用于按钮色彩、展开箭头旋转、行悬停背景微调。
- **悬停反馈**：仅允许背景明度微调（`var(--dsw-alias-interactive-bg-hover)`）与文字下划线，严禁 `hover:scale`。
- **无障碍与减少动效**：保留 `aria-disabled` 聚焦与键盘方向键导航，`prefers-reduced-motion: reduce` 禁用过渡动画。

## 7. 防平庸校验声明
- 无紫蓝渐变、无背景装饰光晕球、无滥用彩色圆底图标、无占位假文案。
- 无纯黑 `#000000` / 纯白 `#FFFFFF` 主画布。
- 无套路化居中 Hero + 三栏卡片布局，无 `hover:scale` 放大动画。
- 所有正文/结果说明受 `max-width: 65ch` 约束，所有数字与计时已启用 `tabular-nums`。
- 本设计文档已落盘，后续 `DoctorPage.module.css` 与 `Row.tsx` 改动 100% 溯源本规范。
