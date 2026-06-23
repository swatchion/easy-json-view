# Easy Json View 视觉与功能升级说明

> 本文档记录在 **原始生产版本**（左侧标题栏 + 历史记录、右侧工具栏 + 输出/输入双栏）基础上完成的品牌 Logo 设计与一系列功能 / 视觉优化，便于评审与开发交接。

---

## 目录

1. [品牌 Logo](#一品牌-logo)
2. [布局与结构](#二布局与结构)
3. [视觉系统](#三视觉系统)
4. [功能增强](#四功能增强)
5. [关键技术实现](#五关键技术实现)
6. [设计令牌（颜色）](#六设计令牌颜色)

---

## 一、品牌 Logo

### 设计概念

**方案：花括号 + 值节点（Braces · Values）**

- 以 JSON 的花括号 `{ }` 作为主体，直读性最强；
- 花括号之间是一列「格式化后的值节点」，中段高亮一颗 **青色节点**，呼应工具内的语法高亮与「格式化 · 查看」语义；
- 蓝色圆角应用图标底（线性渐变），白色图形，深浅主题下均适用。

### 构成规范

| 元素 | 取值 |
| --- | --- |
| 底色渐变 | `#3b78ff → #1f53e0`（135°，左上→右下） |
| 圆角半径 | 图标边长的 25%（48 视图下 `rx=12`，favicon `rx=13`） |
| 图形描边 | `#ffffff`，`stroke-width` 2.4（小尺寸加粗至 3） |
| 高亮节点 | `#22d3ee`（青色），居中，直径最大 |
| 次级节点 | `#ffffff`，`opacity 0.85` |

### 原始 SVG —— 应用图标（主用）

```svg
<svg width="48" height="48" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="ejv" x1="0" y1="0" x2="48" y2="48" gradientUnits="userSpaceOnUse">
      <stop stop-color="#3b78ff"/>
      <stop offset="1" stop-color="#1f53e0"/>
    </linearGradient>
  </defs>
  <rect width="48" height="48" rx="12" fill="url(#ejv)"/>
  <path d="M21 14 C18 14 18 18 18 21 C18 23.5 16 24 14.5 24 C16 24 18 24.5 18 27 C18 30 18 34 21 34"
        stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M27 14 C30 14 30 18 30 21 C30 23.5 32 24 33.5 24 C32 24 30 24.5 30 27 C30 30 30 34 27 34"
        stroke="#fff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
  <circle cx="24" cy="18.4" r="1.7" fill="#fff" opacity=".85"/>
  <circle cx="24" cy="24" r="2.8" fill="#22d3ee"/>
  <circle cx="24" cy="29.6" r="1.7" fill="#fff" opacity=".85"/>
</svg>
```

### 原始 SVG —— 单色符号（用于浅色背景 / 字标并列）

```svg
<svg width="48" height="48" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
  <path d="M21 14 C18 14 18 18 18 21 C18 23.5 16 24 14.5 24 C16 24 18 24.5 18 27 C18 30 18 34 21 34"
        stroke="#2f6bff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M27 14 C30 14 30 18 30 21 C30 23.5 32 24 33.5 24 C32 24 30 24.5 30 27 C30 30 30 34 27 34"
        stroke="#2f6bff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
  <circle cx="24" cy="18.4" r="1.7" fill="#2f6bff" opacity=".5"/>
  <circle cx="24" cy="24" r="2.8" fill="#06b6d4"/>
  <circle cx="24" cy="29.6" r="1.7" fill="#2f6bff" opacity=".5"/>
</svg>
```

### 原始 SVG —— Favicon（小尺寸优化：加粗描边、保留单颗高亮节点）

```svg
<svg width="32" height="32" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="fav" x1="0" y1="0" x2="48" y2="48" gradientUnits="userSpaceOnUse">
      <stop stop-color="#3b78ff"/>
      <stop offset="1" stop-color="#1f53e0"/>
    </linearGradient>
  </defs>
  <rect width="48" height="48" rx="13" fill="url(#fav)"/>
  <path d="M20 14C17 14 17 18 17 21C17 23.5 15 24 13.5 24C15 24 17 24.5 17 27C17 30 17 34 20 34"
        stroke="#fff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M28 14C31 14 31 18 31 21C31 23.5 33 24 34.5 24C33 24 31 24.5 31 27C31 30 31 34 28 34"
        stroke="#fff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
  <circle cx="24" cy="24" r="3.4" fill="#22d3ee"/>
</svg>
```

### Favicon 内联用法（data-uri，已接入页面 `<head>`）

```html
<link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 48 48'><defs><linearGradient id='g' x1='0' y1='0' x2='48' y2='48' gradientUnits='userSpaceOnUse'><stop stop-color='%233b78ff'/><stop offset='1' stop-color='%231f53e0'/></linearGradient></defs><rect width='48' height='48' rx='13' fill='url(%23g)'/><path d='M20 14C17 14 17 18 17 21C17 23.5 15 24 13.5 24C15 24 17 24.5 17 27C17 30 17 34 20 34' stroke='%23fff' stroke-width='3' stroke-linecap='round' stroke-linejoin='round'/><path d='M28 14C31 14 31 18 31 21C31 23.5 33 24 34.5 24C33 24 31 24.5 31 27C31 30 31 34 28 34' stroke='%23fff' stroke-width='3' stroke-linecap='round' stroke-linejoin='round'/><circle cx='24' cy='24' r='3.4' fill='%2322d3ee'/></svg>">
```

> 字标锁定：图标 + `Easy`（深色 `--text`）`Json View`（强调色 `--accent`），字重 800、字距 `-0.022em`，副标题「JSON 格式化 · 树形查看工具」。

---

## 二、布局与结构

| 项 | 原始生产版本 | 升级后 |
| --- | --- | --- |
| 整体骨架 | 左：标题 + 历史；右：工具栏 + 输出/输入 | **保持不变** |
| 语言切换器 | 顶部工具栏中部（中/EN 文本） | 移至 **标题区**，改为「🌐 地球图标 + 当前语言 + 下拉箭头」的下拉菜单（中文 ✓ / English） |
| 输出 / 输入宽度 | 输出弹性、输入固定宽 | 二者按 **2 : 1** 瓜分「除左侧边栏以外」的宽度 |
| 输出 / 输入对齐 | 输出区上方多出统计/搜索/工具行，显示区被压低 | 两面板统一为 **「标题行 + 控制行 + 内容区 + 底部状态栏」**，使「格式化结果」显示区与「JSON 输入」输入区 **顶部、底部均水平对齐** |
| 统计信息 | 标题下方独立成行 | 下沉到输出区 **底部状态栏**，与输入区「字符数 / 行数」状态栏左右对齐 |

---

## 三、视觉系统

- **字体**：界面 `system-ui`；JSON 内容与等宽元素采用 **JetBrains Mono**（Google Fonts）。
- **主题**：浅色 / 深色双主题，通过 CSS 自定义属性（`data-theme` 切换），可在工具栏即时切换。
- **语法配色**：键 / 字符串 / 数字 / 布尔 / Null / 标点 各有独立色值（深浅主题分别定义），搜索命中以 `<mark>` 高亮。
- **控件**：iOS 风格分段控件（字号、主题、文本/树、Aa 区分大小写、语言）；工具按钮统一加 **图标**（清空 ⊗、压缩 ⤢、格式化 ✦、复制、下载、示例、导入等）。
- **细节**：自定义滚动条、卡片化历史记录、强调色高亮、`title` 工具提示、轻量 Toast 反馈。

---

## 四、功能增强

> 以下为相对原始生产版本新增 / 强化的能力。

### 4.1 大整数精度（按文本渲染）
- 原始：超出 JS 安全整数范围的长数字（如 `tid`、`oid`）解析后丢失精度（`…7005000`）。
- 现在：自定义解析在文本层捕获大整数并以原文保留，树形中**以不带引号的大整数样式**显示完整数字（`3304223868037004874`），统计仍计入「数字」。

### 4.2 搜索导航
- 命中计数 **`当前 / 总数`**（如 `1 / 2`）；
- **上一个 / 下一个** 按钮，键盘 **Enter / Shift+Enter** 跳转；
- 跳转时**自动展开**命中所在的折叠节点并**滚动居中**；
- 当前命中行以强调色背景 + 左侧色条高亮，关键字 `<mark>` 高亮；支持 **Aa 区分大小写**。

### 4.3 节点级操作
- 树中每行 hover 浮出 **「复制值」「复制路径」**；
- 复制路径输出 JSONPath 风格表达式（如 `$.data.items[0].id`），并以 Toast 回显。

### 4.4 折叠预览
- 折叠的对象内联预览前几个键（`{ id, name, status, … }`）；数组显示元素数量徽标，扫读更快。

### 4.5 缩进引导线 + 行高亮
- 每层嵌套绘制竖直引导线，深层结构更易对位；
- 点击任意行高亮当前行。

### 4.6 拖拽导入
- 将 `.json` 文件拖拽到**窗口任意位置**即导入，全屏显示「松开以导入 JSON 文件」提示，松手即载入并写入历史；阻止浏览器默认打开文件；
- 「JSON 输入」底部状态栏常驻一句简短提示「支持拖拽 .json 文件到窗口导入」。

### 4.7 自适应
- 顶部工具栏、底部统计栏在窄屏下改为**横向滚动**，不再裁切控件，并保持对齐。

### 4.8 既有能力（一并保留 / 完善）
- 缩进（2 / 4 / Tab）、排序键、字号（小/中/大）、自动格式化、压缩、复制、下载、示例、导入；
- 文本 / 树 双视图；全部展开 / 全部折叠 / 折叠二级；
- 历史记录：本地持久化（localStorage）、搜索、书签收藏、只看书签、点击恢复、清空；
- 快捷键 **⌘/Ctrl + Enter** 格式化；
- 中英双语界面（随语言切换器切换）。

---

## 五、关键技术实现

### 5.1 大整数保留解析
预扫描原始文本（跳过字符串内部），将超出安全范围的整数 token 包裹为带私有区哨兵字符（`U+E000`）的占位串，交由 `JSON.parse` 解析后再还原为 `{ __big:true, raw:"…" }` 标记对象；树渲染、统计、文本视图（自定义 `stringify`）均按数字类型处理并以原文输出，**不丢精度、显示无引号**。

### 5.2 搜索匹配与导航
按树的先序遍历收集所有命中节点路径，维护 `当前索引`；跳转时移除路径上所有祖先的折叠态以展开命中项，并在 `componentDidUpdate` 中按目标行 `offsetTop` 将滚动容器滚动居中。

### 5.3 全窗口拖拽
在 `window` 上监听 `dragenter / dragover / dragleave / drop`，以 `Files` 类型判定 + 进出计数控制全屏提示显隐，`drop` 时 `preventDefault` 并读取文件，避免浏览器导航离开页面。

---

## 六、设计令牌（颜色）

| Token | 浅色 | 深色 |
| --- | --- | --- |
| `--app-bg` | `#e9edf3` | `#0a0e15` |
| `--panel` | `#ffffff` | `#121823` |
| `--panel-2` | `#f6f8fc` | `#0d131c` |
| `--border` | `#e2e8f1` | `#222c3a` |
| `--text` | `#16202f` | `#e6edf6` |
| `--muted` | `#5d6b80` | `#9aa7b8` |
| `--accent` | `#2f6bff` | `#4f8bff` |
| `--accent-soft` | `#eaf0ff` | `#172741` |
| `--guide`（引导线） | `#e7ecf3` | `#232c3a` |
| 语法 · 键 `--sx-key` | `#0b62d6` | `#79c0ff` |
| 语法 · 字符串 `--sx-str` | `#1d8f4e` | `#7ee787` |
| 语法 · 数字 `--sx-num` | `#b4530c` | `#ffa657` |
| 语法 · 布尔 `--sx-bool` | `#8a3ffc` | `#d2a8ff` |
| 语法 · Null `--sx-null` | `#7a8699` | `#8b949e` |
| 高亮节点（Logo） | `#22d3ee` | `#22d3ee` |

---

*文档随 `Easy Json View.dc.html` 当前实现整理；Logo 概念稿见 `Easy Json View Logo.dc.html`。*
