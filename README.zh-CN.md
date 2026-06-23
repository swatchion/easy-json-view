[English](./README.md) · **简体中文**

# EazyJsonView

一个现代化的 JSON 格式化 / 校验工具，使用 **Rust + Dioxus 0.7.9** 编写，**同时面向桌面与 Web 双目标**：

- **桌面**（默认）：原生窗口应用（wry/webkit2gtk），无浏览器、无服务器；历史与配置存用户配置目录下的单个 `store.json`。
- **Web**：编译为 **WebAssembly** 在浏览器运行；历史与配置存浏览器 `localStorage`。

核心 JSON 逻辑与全部功能两端共用，仅「存储」与少数「DOM/系统」辅助函数按平台分叉。

## ✨ 特性

- 🚀 **高性能**：基于 Rust + WebAssembly，处理速度快；输出采用单文本块 + 行号渲染，大文档下依然流畅
- 🎨 **格式化（美化）**：可选 2 / 4 / 8 空格缩进
- 🌈 **语法高亮**：对中小规模输出着色（键 / 字符串 / 数字 / 布尔 / null），超大文档自动回退为纯文本以保持性能
- 🗜️ **压缩（Minify）**：去除空白，压缩为单行
- 🔤 **键序控制**：默认保留对象原始键序，可选「排序键」按字母序排序
- 🔎 **结果区键值搜索**：在格式化结果中查找键/值，高亮全部匹配并显示计数，支持上一个 / 下一个跳转（Enter / Shift+Enter）与区分大小写；超大文档下匹配数封顶以保持性能
- 📊 **JSON 统计**：统计对象 / 数组 / 键 / 字符串 / 数字 / 布尔 / Null 及值总数
- 💾 **历史记录**（localStorage）：自动保存、搜索、重命名、删除、清空；按内容去重，最多保留 100 条
- ⭐ **书签收藏**：标记重要记录后置顶显示且永不被淘汰，支持「只看书签」过滤；重新格式化相同内容仍保留书签
- 📋 **复制 / 下载 / 导入**：一键复制输出或输入、下载输出为 `.json`、从本地文件导入 JSON
- ⚠️ **错误提示**：内联错误横幅，给出 serde 的行 / 列定位信息
- ⌨️ **快捷键**：`Ctrl+Enter`（或 `Cmd+Enter`）快速格式化
- 🧩 **示例 / 清空**：一键填入内置示例、清空输入与输出
- ⚙️ **设置持久化**：缩进大小与排序键选项跨刷新保存
- 📱 **响应式 & 无障碍**：移动端自动堆叠，提供 `aria-label` 与焦点环

## 🛠️ 技术栈

- **UI 框架**：Dioxus 0.7.9（`desktop` / `web` 双 renderer，按目标二选一）
- **编程语言**：Rust（edition 2021）
- **编译目标**：原生（桌面）/ WebAssembly（`wasm32-unknown-unknown`，Web）
- **数据存储**：桌面 `~/.config/eazy-json-view/store.json` 单文件 / Web 浏览器 `localStorage`（由 `src/platform/` 垫片按平台路由）
- **样式**：Tailwind CSS（**离线**，standalone CLI 生成 `assets/tailwind.css`，经 `asset!` + `document::Stylesheet` 引入；不再用 CDN）
- **构建工具**：`dx`（dioxus-cli）
- **JSON 处理**：`serde_json`（启用 `preserve_order` 以保留键序）

## 🚀 快速开始

### 构建前置

```bash
# dx CLI（一次性）；版本须与 Cargo.toml 的 dioxus 一致
cargo install dioxus-cli --version 0.7.9 --locked

# 桌面端系统依赖（wry/webkit2gtk）
sudo dnf install webkit2gtk4.1-devel libsoup3-devel     # Fedora
# sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev # Debian/Ubuntu

# Web 端目标
rustup target add wasm32-unknown-unknown
```

> Tailwind standalone CLI 由构建脚本自动下载（`./tailwindcss`，已加入 `.gitignore`）。

### 桌面版（原生窗口）

```bash
./build-desktop.sh            # 生成样式 → 逻辑测试 → dx serve（弹出原生窗口）
./build-desktop.sh build      # 仅构建可执行产物
# 等价手动：dx serve --platform desktop
```

### Web 版

```bash
./build-web.sh                # 生成样式 → dx build --platform web → 逻辑测试
./build-web.sh serve          # 本地预览（dx serve --platform web）
```

Web 静态产物位于 `target/dx/eazy-json-view/release/web/public/`，可用任意静态服务器托管。

> 说明：release 构建末尾 `wasm-opt` 优化步骤可能因调试信息（DWARF）崩溃并打印 `ERROR ... wasm-opt failed`，dx 会自动回退到未优化的 wasm——构建仍成功、可正常运行，仅体积偏大（dx 0.7 已知现象，非构建失败）。

> 注意：`cargo build`/`test`/`bench` 默认走 desktop 特性（需 webkit）；纯逻辑工作可加 `--no-default-features`。

## 📁 项目结构

```
├── LICENSE                         # PolyForm Noncommercial 1.0.0（前言豁免 + 逐字原文）
├── README.md                       # 项目主文档（英文，默认）
├── README.zh-CN.md                 # 项目主文档（简体中文）
├── index.html                      # Web 页面骨架（仅防闪烁脚本 + 挂载点；CDN 与加载屏已移除）
├── Dioxus.toml                     # dx 配置（[web.app] title 等）
├── tailwind.config.js              # Tailwind content 扫描 src/**/*.rs + index.html
├── build-desktop.sh / build-web.sh # 桌面 / Web 构建脚本（替代旧 build.sh + serve.py）
├── Cargo.toml                      # 双目标依赖与 [features]（default=desktop / web / desktop）
├── assets/
│   ├── input.css                   # Tailwind 输入（@tailwind + 全局/深色覆盖）
│   └── tailwind.css                # 生成的离线样式（asset! 引入）
├── src/
│   ├── main.rs                     # 入口（cfg 分支：wasm launch / 桌面 LaunchBuilder+窗口）
│   ├── lib.rs                      # 库入口，向 benches/tests 暴露 services；挂 platform
│   ├── app.rs                      # 全部 UI（cfg 分叉的平台辅助函数）
│   ├── platform/                   # 缝1：Storage 平台垫片
│   │   ├── mod.rs                  #   按 cfg 路由
│   │   ├── web.rs                  #   localStorage（gloo）
│   │   └── desktop.rs              #   store.json 单文件
│   ├── services/
│   │   └── mod_enhanced.rs         # 全部业务逻辑与类型（平台无关）
│   └── tests.rs                    # 单元测试
└── benches/
    └── json_performance.rs         # Criterion 基准测试
```

- `app.rs`：整个界面与事件处理，RSX 中直接书写 Tailwind class；平台相关辅助（剪贴板/下载/计时/文件导入）按 `cfg` 分叉但保持签名。
- `platform/`：`Storage::{get,set,delete}` 同构垫片，Web→localStorage，桌面→`store.json`。
- `services/mod_enhanced.rs`（平台无关）：
  - `JsonService` — `validate` / `format` / `minify` / `get_stats`（基于 `serde_json`）
  - `HistoryService` — 历史 CRUD（key=`eazy_json_view_history`，去重、上限 100）
  - `ConfigService` — 配置读写（key=`eazy_json_view_config`）
  - 类型：`HistoryRecord`、`FormatOptions`、`ValidationResult`、`JsonStats`、`AppConfig`、`UiSettings`、`TreeRow`

## 💾 数据存储

历史与配置经 `src/platform/` 垫片按平台持久化，无服务器、不上传远程：

- **桌面**：单个 JSON 文件 `~/.config/eazy-json-view/store.json`，内含 `eazy_json_view_history` 与 `eazy_json_view_config` 两个键。
- **Web**：浏览器 `localStorage`，同样两个键。
- `eazy_json_view_history` — 历史列表（按内容去重，最多 100 条；默认名称为内容 SHA1 的前 7 位短 hash，记录创建时间，书签永不淘汰）。
- `eazy_json_view_config` — 格式化选项（缩进 + 排序键）与 UI 设置（主题 / 字号 / 自动格式化）。

## 🧪 测试与基准

```bash
# 纯逻辑单元测试（无需 webkit，推荐）
cargo test --lib --no-default-features

# 单个测试（按名匹配）
cargo test test_json_formatting --no-default-features

# 基准测试（Criterion）
cargo bench --no-default-features
```

## 🗺️ 路线图 / Roadmap

- 桌面打包为可安装产物（AppImage / `dx bundle`）——本次双目标改造未含，留后续
- JSON 字符串转义 / 反转义
- 多语言
- 零闪烁的桌面启动主题（当前首帧可能极短浅色闪烁）

## 📄 许可证

本项目采用 **[PolyForm Noncommercial License 1.0.0](./LICENSE)**（SPDX: `PolyForm-Noncommercial-1.0.0`）。

- **源码可见、禁止商用**：源代码公开可读，允许个人学习、研究、实验，以及非营利组织使用等一切**非商业用途**；任何形式的**商业使用须另行获得授权**。这是一份 source-available（源码可见）非商用许可，**并非 OSI 定义的「开源」许可**。
- **原始开发者不受此限制**：上述非商用限制仅约束被授权方（许可证中的 "you"）；版权人（licensor）保留对本软件的全部权利——包括使用、修改、分发、再许可与商业利用，并可另行向他人授予商业许可。详见 `LICENSE` 顶部的豁免声明（NOTICE）。

完整条款见仓库根目录的 [`LICENSE`](./LICENSE)。
