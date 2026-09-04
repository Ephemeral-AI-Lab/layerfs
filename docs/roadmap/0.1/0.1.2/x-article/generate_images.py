#!/usr/bin/env python3
"""Generate the English and Simplified Chinese v0.1.2 X Article images."""

from __future__ import annotations

import html
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent
WIDTH, HEIGHT = 1600, 900
INK = "#18202b"
MUTED = "#61708a"
GRID = "#dbe2ec"
LIGHT = "#f5f7fb"
PURPLE = "#7c3aed"
PURPLE_LIGHT = "#f2ebff"
BLUE = "#2563eb"
BLUE_LIGHT = "#eaf2ff"
ORANGE = "#ea580c"
ORANGE_LIGHT = "#fff1e8"
GREEN = "#16845b"
GREEN_LIGHT = "#e9f8f1"
RED = "#cf334f"
RED_LIGHT = "#fff0f3"


COPY = {
    "en": {
        "font": "Arial, sans-serif",
        "suffix": "",
        "locality": {
            "title": "Imagine adding one page to the front of a giant book",
            "subtitle": "The slow idea rewrites the book. LayerFS changes the table of contents.",
            "old": "Slow idea: rewrite every old page",
            "old_sub": "A tiny change becomes giant work",
            "new": "LayerFS idea: keep the pages, change the pointers",
            "new_sub": "A tiny change stays tiny",
            "new_bytes": "NEW PAGE\n4 KiB",
            "move": "REWRITE THE UNCHANGED BOOK\n100 MiB",
            "inline": "NEW PAGE\n4 KiB",
            "base": "POINTER TO THE SAME OLD BOOK\n100 MiB",
            "old_publish": "save the whole book",
            "new_publish": "save new page + pointers",
            "zero": "Old pages moved: 0",
            "footer": "Same final book. Less work.",
        },
        "pipeline": {
            "title": "A draft list and a saved tree make the trick work",
            "subtitle": "One list tracks the draft. One tree publishes the saved version.",
            "boxes": [
                ("1. Ask for an edit", "where · remove how much\nadd what"),
                ("2. Update the draft", "old piece · new piece\nold piece"),
                ("3. Keep the final list", "ignore edits that were\noverwritten later"),
                ("4. Reuse old pages", "save only new pages\nand changed pointers"),
                ("5. Publish", "one new version\nold versions still work"),
            ],
            "fuse": "Refresh one open file—not the whole mounted workspace",
            "fuse_sub": "v0.1.2 keeps the running workspace in place",
            "foundation": "the older extent-tree foundation",
            "release": "new in v0.1.2",
            "claim": "The work follows what changed—not every byte that stayed the same.",
        },
        "scaling": {
            "title": "The book grows 500×. The tiny edit stays fast.",
            "subtitle": "Add 4 KiB at the front · LayerFS Edit + Commit median · N=5",
            "x": "Committed file size (MiB)",
            "y": "Latency (ms)",
            "edit": "",
            "commit": "",
            "combined": "Edit + Commit",
            "note": "Measured on exact 1 / 10 / 100 / 500 MiB files. Bigger files still add Store and metadata work.",
            "callout": "500 MiB file\n14.300 ms",
        },
        "cloudflare": {
            "title": "One 100 MiB file. Three tiny 4 KiB edits.",
            "subtitle": "How long until the changed file is published?",
            "x": "",
            "layerfs": "LayerFS",
            "cloudflare": "Pinned Cloudflare Computer path",
            "ops": ["Overwrite", "Insert in middle", "Add at the front"],
            "ratios": ["33×", "392×", "803×"],
            "caveat": "Different APIs and timing boundaries. These are complete measured paths—not a universal product-speed claim.",
            "hero": "≈803×",
        },
        "evidence": {
            "title": "We did not test one lucky edit",
            "subtitle": "Every speed run had a separate correctness check",
            "cards": [("56", "different edit cases"), ("560", "timed runs"), ("112", "correctness proofs")],
            "family": "What changed?",
            "cases": "Cases",
            "samples": "Samples",
            "proofs": "Proofs",
            "families": [
                ("Bytes changed; length stayed", 12, 120, 24),
                ("File became longer or shorter", 32, 320, 64),
                ("The saved tree changed shape", 12, 120, 24),
            ],
            "support": "Plus namespace and storage-footprint checks",
            "footer": "Each edit was tested on 1 / 10 / 100 / 500 MiB files",
        },
        "complexity": {
            "title": "Big-O, without the scary part",
            "subtitle": "N old file · a/A changed bytes · H height · P pieces · T tree work · S system work",
            "headers": ("Job", "Cost", "Plain-English meaning"),
            "rows": [
                ("Copy-based edit", "Θ(N + a)", "Rewrite the old file and add new bytes"),
                ("LayerFS draft edit", "O(a + H + D)", "Add bytes; change a tree path; remove old pieces"),
                ("LayerFS Commit", "O(P + A + T) + S", "Walk final pieces; save changed data and pointers"),
                ("Read or hash the whole file", "Θ(N)", "Still touch every byte—there is no shortcut"),
            ],
            "footer": "LayerFS removes the forced full-file rewrite. It does not make every filesystem operation O(1).",
        },
    },
    "zh-CN": {
        "font": "Hiragino Sans GB, Arial Unicode MS, sans-serif",
        "suffix": "-zh-CN",
        "locality": {
            "title": "想象在一本巨厚的书前面加一页",
            "subtitle": "慢办法重写整本书；LayerFS 只修改目录卡片。",
            "old": "慢办法：重写每一张旧页",
            "old_sub": "一个小修改变成巨大工作量",
            "new": "LayerFS：保留旧页，只修改指针",
            "new_sub": "小修改继续保持小",
            "new_bytes": "新页面\n4 KiB",
            "move": "重写未变化的整本书\n100 MiB",
            "inline": "新页面\n4 KiB",
            "base": "指向原书的引用\n100 MiB",
            "old_publish": "保存整本书",
            "new_publish": "保存新页和新指针",
            "zero": "被搬动的旧页：0",
            "footer": "最后得到同一本书，但工作量少得多。",
        },
        "pipeline": {
            "title": "一张草稿清单和一棵保存树让技巧成为可能",
            "subtitle": "一张记录草稿，一棵树发布保存后的版本。",
            "boxes": [
                ("1. 提出修改", "在哪里 · 删除多少\n加入什么"),
                ("2. 更新草稿", "旧片段 · 新片段\n旧片段"),
                ("3. 保留最终清单", "忽略后来又被覆盖的\n中间修改"),
                ("4. 复用旧页面", "只保存新页面\n和变化指针"),
                ("5. 发布", "得到一个新版本\n旧版本仍然可用"),
            ],
            "fuse": "只刷新一个打开的文件，不重建整个 Workspace 挂载",
            "fuse_sub": "v0.1.2 让正在运行的 Workspace 留在原地",
            "foundation": "更早的 extent-tree 基础",
            "release": "v0.1.2 新增",
            "claim": "工作量跟随变化内容，而不是跟随所有未变化字节。",
        },
        "scaling": {
            "title": "书变大 500 倍，小修改仍然很快",
            "subtitle": "在开头加入 4 KiB · LayerFS Edit + Commit 中位数 · N=5",
            "x": "已提交文件大小（MiB）",
            "y": "延迟（ms）",
            "edit": "",
            "commit": "",
            "combined": "Edit + Commit",
            "note": "精确测试 1 / 10 / 100 / 500 MiB 文件；更大文件仍会增加 Store 与元数据工作。",
            "callout": "500 MiB 文件\n14.300 ms",
        },
        "cloudflare": {
            "title": "同一个 100 MiB 文件，三次很小的 4 KiB 修改",
            "subtitle": "修改后的文件需要多久才能发布？",
            "x": "",
            "layerfs": "LayerFS",
            "cloudflare": "固定 Cloudflare Computer 路径",
            "ops": ["覆盖", "在中间插入", "在开头插入"],
            "ratios": ["33×", "392×", "803×"],
            "caveat": "API 与计时边界不同；这里比较完整已测路径，不代表普遍产品速度。",
            "hero": "约 803×",
        },
        "evidence": {
            "title": "我们没有只测一个幸运用例",
            "subtitle": "每次速度测试都有独立的正确性检查",
            "cards": [("56", "种不同编辑"), ("560", "次计时运行"), ("112", "条正确性证明")],
            "family": "什么发生了变化？",
            "cases": "用例",
            "samples": "样本",
            "proofs": "证明",
            "families": [
                ("字节变了，长度没变", 12, 120, 24),
                ("文件变长或变短", 32, 320, 64),
                ("保存后的树改变形状", 12, 120, 24),
            ],
            "support": "另外还检查了 namespace 与存储占用",
            "footer": "每种编辑都在 1 / 10 / 100 / 500 MiB 文件上测试",
        },
        "complexity": {
            "title": "不吓人的 Big-O 表",
            "subtitle": "N 原文件 · a/A 变化字节 · H 树高 · P 片段 · T 树工作 · S 系统工作",
            "headers": ("要做的事", "成本", "人话解释"),
            "rows": [
                ("复制式编辑", "Θ(N + a)", "重写原文件，再加入新字节"),
                ("LayerFS 草稿编辑", "O(a + H + D)", "加入字节、修改树路径、删除旧片段"),
                ("LayerFS Commit", "O(P + A + T) + S", "遍历最终片段，只保存变化数据和指针"),
                ("读取或哈希整个文件", "Θ(N)", "仍要访问每个字节，没有捷径"),
            ],
            "footer": "LayerFS 删除强制的整文件重写，但不会让所有文件系统操作都变成 O(1)。",
        },
    },
}


def esc(value: object) -> str:
    return html.escape(str(value), quote=True)


def text(x: float, y: float, value: str, *, size: int = 26, weight: int = 400,
         fill: str = INK, anchor: str = "start", family: str = "Arial, sans-serif",
         line_height: float = 1.25) -> str:
    lines = value.split("\n")
    spans = []
    for index, line in enumerate(lines):
        dy = 0 if index == 0 else size * line_height
        spans.append(f'<tspan x="{x}" dy="{dy}">{esc(line)}</tspan>')
    return (
        f'<text x="{x}" y="{y}" font-family="{esc(family)}" font-size="{size}" '
        f'font-weight="{weight}" fill="{fill}" text-anchor="{anchor}">' + "".join(spans) + "</text>"
    )


def rect(x: float, y: float, w: float, h: float, *, fill: str = "white",
         stroke: str = GRID, radius: int = 18, stroke_width: int = 2) -> str:
    return (
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" '
        f'fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}"/>'
    )


def line(x1: float, y1: float, x2: float, y2: float, *, stroke: str = MUTED,
         width: int = 3, arrow: bool = False, dash: str | None = None) -> str:
    extra = ' marker-end="url(#arrow)"' if arrow else ""
    if dash:
        extra += f' stroke-dasharray="{dash}"'
    return f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{stroke}" stroke-width="{width}"{extra}/>'


def svg_document(content: str, family: str) -> str:
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">
<defs>
  <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="{MUTED}"/></marker>
</defs>
<rect width="{WIDTH}" height="{HEIGHT}" fill="white"/>
<g font-family="{esc(family)}">{content}</g>
</svg>'''


def header(copy: dict[str, object], family: str) -> str:
    return text(90, 80, str(copy["title"]), size=50, weight=700, family=family) + text(
        90, 122, str(copy["subtitle"]), size=24, fill=MUTED, family=family
    )


def locality_svg(c: dict[str, object], family: str) -> str:
    out = [header(c, family)]
    rows = [
        (180, c["old"], c["old_sub"], ORANGE, ORANGE_LIGHT, c["new_bytes"], c["move"], c["old_publish"]),
        (485, c["new"], c["new_sub"], PURPLE, PURPLE_LIGHT, c["inline"], c["base"], c["new_publish"]),
    ]
    for y, title_value, subtitle, color, light, small, large, publish in rows:
        out += [text(90, y, str(title_value), size=31, weight=700, family=family),
                text(90, y + 38, str(subtitle), size=21, fill=MUTED, family=family)]
        out += [rect(90, y + 70, 210, 120, fill=light, stroke=color),
                text(195, y + 118, str(small), size=24, weight=700, anchor="middle", family=family),
                rect(316, y + 70, 930, 120, fill=light, stroke=color),
                text(781, y + 118, str(large), size=25, weight=700, anchor="middle", family=family),
                line(1260, y + 130, 1330, y + 130, arrow=True),
                rect(1345, y + 82, 165, 96, fill=LIGHT, stroke=GRID),
                text(1427, y + 122, str(publish), size=17, weight=600, anchor="middle", family=family)]
    out += [rect(1080, 724, 430, 70, fill=GREEN_LIGHT, stroke=GREEN, radius=14),
            text(1295, 768, str(c["zero"]), size=23, weight=700, fill=GREEN, anchor="middle", family=family),
            text(90, 840, str(c["footer"]), size=22, fill=MUTED, family=family)]
    return svg_document("".join(out), family)


def pipeline_svg(c: dict[str, object], family: str) -> str:
    out = [header(c, family)]
    xs = [70, 375, 680, 985, 1290]
    fills = [BLUE_LIGHT, PURPLE_LIGHT, ORANGE_LIGHT, GREEN_LIGHT, BLUE_LIGHT]
    colors = [BLUE, PURPLE, ORANGE, GREEN, BLUE]
    for index, ((title_value, subtitle), x) in enumerate(zip(c["boxes"], xs)):
        out += [rect(x, 260, 240, 180, fill=fills[index], stroke=colors[index]),
                text(x + 120, 318, str(title_value), size=23, weight=700, anchor="middle", family=family),
                text(x + 120, 365, str(subtitle), size=18, fill=MUTED, anchor="middle", family=family)]
        if index < 4:
            out.append(line(x + 245, 350, x + 294, 350, arrow=True))
    out += [line(495, 215, 495, 250, stroke=PURPLE, arrow=True),
            text(350, 195, str(c["release"]), size=18, weight=700, fill=PURPLE, family=family),
            line(1105, 215, 1105, 250, stroke=GREEN, arrow=True),
            text(985, 195, str(c["foundation"]), size=18, weight=700, fill=GREEN, family=family),
            line(495, 450, 495, 535, arrow=True),
            rect(350, 550, 1030, 120, fill=LIGHT, stroke=PURPLE),
            text(865, 600, str(c["fuse"]), size=30, weight=700, anchor="middle", family=family),
            text(865, 640, str(c["fuse_sub"]), size=22, fill=MUTED, anchor="middle", family=family),
            rect(130, 740, 1340, 90, fill=GREEN_LIGHT, stroke=GREEN, radius=14),
            text(800, 795, str(c["claim"]), size=24, weight=700, fill=GREEN, anchor="middle", family=family)]
    return svg_document("".join(out), family)


def scaling_svg(c: dict[str, object], family: str) -> str:
    out = [header(c, family)]
    left, top, right, bottom = 150, 205, 1250, 720
    sizes = [1, 10, 100, 500]
    combined = [4.680, 4.883, 7.257, 14.300]
    x_positions = [left + index * (right - left) / 3 for index in range(4)]
    y = lambda value: bottom - value / 16 * (bottom - top)
    for tick in [0, 4, 8, 12, 16]:
        py = y(tick)
        out += [line(left, py, right, py, stroke=GRID, width=2),
                text(left - 22, py + 8, str(tick), size=18, fill=MUTED, anchor="end", family=family)]
    out += [line(left, top, left, bottom, stroke=MUTED, width=2), line(left, bottom, right, bottom, stroke=MUTED, width=2)]
    for x, label in zip(x_positions, sizes):
        out += [line(x, bottom, x, bottom + 8, stroke=MUTED, width=2),
                text(x, bottom + 40, str(label), size=20, fill=MUTED, anchor="middle", family=family)]
    points = " ".join(f"{x},{y(value)}" for x, value in zip(x_positions, combined))
    out.append(f'<polyline points="{points}" fill="none" stroke="{PURPLE}" stroke-width="8" stroke-linejoin="round"/>')
    for x, value in zip(x_positions, combined):
        out += [f'<circle cx="{x}" cy="{y(value)}" r="11" fill="{PURPLE}"/>',
                text(x, y(value) - 22, f"{value:.3f} ms", size=21, weight=700, fill=PURPLE, anchor="middle", family=family)]
    out += [text((left + right) / 2, 815, str(c["x"]), size=21, fill=MUTED, anchor="middle", family=family),
            f'<text x="52" y="470" transform="rotate(-90 52 470)" font-family="{esc(family)}" '
            f'font-size="21" fill="{MUTED}" text-anchor="middle">{esc(c["y"])}</text>']
    out += [line(1300, 245, 1342, 245, stroke=PURPLE, width=7),
            text(1356, 252, str(c["combined"]), size=22, weight=700, family=family),
            rect(1275, 430, 260, 115, fill=PURPLE_LIGHT, stroke=PURPLE),
            text(1405, 475, str(c["callout"]), size=22, weight=700, anchor="middle", family=family),
            text(90, 865, str(c["note"]), size=18, fill=MUTED, family=family)]
    return svg_document("".join(out), family)


def cloudflare_svg(c: dict[str, object], family: str) -> str:
    out = [header(c, family)]
    layerfs = [6.928, 7.752, 7.257]
    cloudflare = [225.759, 3040.892, 5827.631]
    for index, (op, ratio) in enumerate(zip(c["ops"], c["ratios"])):
        x = 80 + index * 510
        color = RED if index == 2 else ORANGE
        light = RED_LIGHT if index == 2 else ORANGE_LIGHT
        out += [rect(x, 190, 460, 520, fill="white", stroke=GRID, radius=18),
                text(x + 230, 255, str(op), size=32, weight=700, anchor="middle", family=family),
                rect(x + 40, 305, 380, 110, fill=PURPLE_LIGHT, stroke=PURPLE, radius=14),
                text(x + 65, 347, str(c["layerfs"]), size=20, weight=700, fill=PURPLE, family=family),
                text(x + 395, 380, f"{layerfs[index]:.3f} ms", size=30, weight=700, fill=PURPLE, anchor="end", family=family),
                rect(x + 40, 445, 380, 125, fill=light, stroke=color, radius=14),
                text(x + 65, 487, str(c["cloudflare"]), size=18, weight=700, fill=color, family=family),
                text(x + 395, 535, f"{cloudflare[index]:,.1f} ms", size=30, weight=700, fill=color, anchor="end", family=family),
                text(x + 230, 645, str(ratio), size=48, weight=700, fill=color, anchor="middle", family=family)]
    out += [rect(160, 760, 1280, 70, fill=LIGHT, stroke=GRID, radius=12),
            text(800, 804, str(c["caveat"]), size=19, fill=MUTED, anchor="middle", family=family)]
    return svg_document("".join(out), family)


def evidence_svg(c: dict[str, object], family: str) -> str:
    out = [header(c, family)]
    colors = [PURPLE, BLUE, GREEN]
    fills = [PURPLE_LIGHT, BLUE_LIGHT, GREEN_LIGHT]
    for index, (value, label) in enumerate(c["cards"]):
        x = 90 + index * 505
        out += [rect(x, 180, 460, 150, fill=fills[index], stroke=colors[index]),
                text(x + 230, 245, str(value), size=58, weight=700, fill=colors[index], anchor="middle", family=family),
                text(x + 230, 294, str(label), size=21, weight=600, anchor="middle", family=family)]
    xcols = [100, 850, 1080, 1310]
    out += [rect(90, 385, 1420, 300, fill="white", stroke=GRID, radius=8),
            text(xcols[0], 430, str(c["family"]), size=22, weight=700, family=family),
            text(xcols[1], 430, str(c["cases"]), size=22, weight=700, anchor="middle", family=family),
            text(xcols[2], 430, str(c["samples"]), size=22, weight=700, anchor="middle", family=family),
            text(xcols[3], 430, str(c["proofs"]), size=22, weight=700, anchor="middle", family=family),
            line(90, 455, 1510, 455, stroke=GRID, width=2)]
    for index, row in enumerate(c["families"]):
        y = 510 + index * 72
        if index % 2:
            out.append(f'<rect x="92" y="{y - 35}" width="1416" height="64" fill="{LIGHT}"/>')
        out += [text(xcols[0], y, str(row[0]), size=22, weight=600, family=family),
                text(xcols[1], y, str(row[1]), size=23, weight=700, anchor="middle", family=family),
                text(xcols[2], y, str(row[2]), size=23, weight=700, anchor="middle", family=family),
                text(xcols[3], y, str(row[3]), size=23, weight=700, anchor="middle", family=family)]
    out += [rect(90, 720, 1420, 65, fill=ORANGE_LIGHT, stroke=ORANGE, radius=12),
            text(800, 761, str(c["support"]), size=20, weight=600, anchor="middle", family=family),
            text(800, 842, str(c["footer"]), size=20, fill=MUTED, anchor="middle", family=family)]
    return svg_document("".join(out), family)


def complexity_svg(c: dict[str, object], family: str) -> str:
    out = [header(c, family)]
    x1, x2, x3, x4 = 80, 555, 995, 1520
    top = 175
    out += [rect(x1, top, x4 - x1, 615, fill="white", stroke=GRID, radius=8),
            f'<rect x="{x1 + 2}" y="{top + 2}" width="{x4 - x1 - 4}" height="72" fill="{INK}"/>',
            text(x1 + 20, top + 47, str(c["headers"][0]), size=22, weight=700, fill="white", family=family),
            text((x2 + x3) / 2, top + 47, str(c["headers"][1]), size=22, weight=700, fill="white", anchor="middle", family=family),
            text(x3 + 20, top + 47, str(c["headers"][2]), size=22, weight=700, fill="white", family=family)]
    row_h = 540 / len(c["rows"])
    for index, row in enumerate(c["rows"]):
        y = top + 73 + index * row_h
        fill = GREEN_LIGHT if index in (1, 2) else (LIGHT if index % 2 else "white")
        out += [f'<rect x="{x1 + 2}" y="{y}" width="{x4 - x1 - 4}" height="{row_h}" fill="{fill}"/>',
                line(x1, y + row_h, x4, y + row_h, stroke=GRID, width=1),
                text(x1 + 20, y + row_h / 2 + 8, str(row[0]), size=20, weight=600, family=family),
                text((x2 + x3) / 2, y + row_h / 2 + 8, str(row[1]), size=19, weight=700,
                     fill=GREEN if index in (1, 2) else INK, anchor="middle", family=family, line_height=1.15),
                text(x3 + 20, y + row_h / 2 + 8, str(row[2]), size=19, family=family)]
    out += [line(x2, top, x2, top + 615, stroke=GRID, width=2),
            line(x3, top, x3, top + 615, stroke=GRID, width=2),
            rect(80, 820, 1440, 58, fill=RED_LIGHT, stroke=RED, radius=10),
            text(800, 857, str(c["footer"]), size=20, weight=700, fill=RED, anchor="middle", family=family)]
    return svg_document("".join(out), family)


GENERATORS = [
    ("01-edit-locality", "locality", locality_svg),
    ("02-workspace-edit-pipeline", "pipeline", pipeline_svg),
    ("03-prepend-scaling", "scaling", scaling_svg),
    ("04-cloudflare-comparison", "cloudflare", cloudflare_svg),
    ("05-evidence-matrix", "evidence", evidence_svg),
    ("06-big-o-table", "complexity", complexity_svg),
]


def render(svg: str, destination: Path) -> None:
    converter = shutil.which("rsvg-convert")
    if converter is None:
        raise SystemExit("rsvg-convert is required")
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", suffix=".svg", encoding="utf-8") as source:
        source.write(svg)
        source.flush()
        subprocess.run(
            [converter, "--width", str(WIDTH), "--height", str(HEIGHT), "--output", str(destination), source.name],
            check=True,
        )


def main() -> None:
    for locale, copy in COPY.items():
        family = str(copy["font"])
        suffix = str(copy["suffix"])
        for basename, key, generator in GENERATORS:
            render(generator(copy[key], family), ROOT / locale / "images" / f"{basename}{suffix}.png")


if __name__ == "__main__":
    main()
