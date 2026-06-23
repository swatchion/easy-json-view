/** @type {import('tailwindcss').Config} */
module.exports = {
  // content 必须同时扫描 src/**/*.rs 与 index.html：
  // 语义色工具类（bg-panel / text-ink / text-str 等）来自 app.rs 与 services/mod_enhanced.rs，
  // 只扫 app.rs 会把 services 产出的语法色类 JIT purge 掉导致树形配色失效。
  content: [
    "./src/**/*.rs",
    "./index.html",
  ],
  theme: {
    extend: {
      // ===== prototype 设计令牌（语义色名）=====
      // 此处仅填【浅色】值；深色由 assets/input.css 的集中式 `html.dark .X { …!important }`
      // 覆盖块按 prototype 深色令牌逐项重映射（沿用本仓库既有深色手法）。
      // 不用 `var(--x)` 是为保留 Tailwind v3 的 alpha 修饰（如 bg-accent/50）正常工作。
      colors: {
        app: "#e9edf3",
        panel: "#ffffff",
        panel2: "#f6f8fc",
        headerbg: "#ffffff",
        line: "#e2e8f1",
        line2: "#eef2f8",
        ink: "#16202f",
        muted: "#5d6b80",
        muted2: "#93a0b3",
        accent: "#2f6bff",
        accentsoft: "#eaf0ff",
        field: "#f3f6fb",
        fieldline: "#dde4ef",
        guide: "#e7ecf3",
        danger: "#e1574e",
        // 语法高亮（键 / 字符串 / 数字 / 布尔 / Null / 标点）
        key: "#0b62d6",
        str: "#1d8f4e",
        num: "#b4530c",
        bool: "#8a3ffc",
        null: "#7a8699",
        punct: "#9aa7b9",
      },
      fontFamily: {
        mono: ["JetBrains Mono", "ui-monospace", "SFMono-Regular", "Menlo", "Consolas", "monospace"],
      },
      borderRadius: {
        // prototype 控件圆角偏大（卡片/分段控件 ~9-11px）
        pill: "999px",
      },
      boxShadow: {
        // prototype 浮层阴影（语言菜单 / Toast / 拖拽遮罩卡片）
        panel: "0 1px 2px rgba(16,24,40,.05), 0 12px 30px -20px rgba(16,24,40,.22)",
      },
    },
  },
  plugins: [],
}
