# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目简介

EazyJsonView 是一个 JSON 格式化/校验工具，用 **Rust + Dioxus 0.7.9** 编写，**同时面向桌面与 Web 双目标**：

- **桌面**（默认目标）：`dx serve --platform desktop` 启动**原生窗口**（wry/webkit2gtk），无浏览器、无 Python。历史与配置存用户配置目录下的单个 `~/.config/eazy-json-view/store.json`。
- **Web**：`dx build --platform web` 编译为 WebAssembly 在浏览器运行，历史与配置存 `localStorage`。

核心 JSON 逻辑与全部功能（格式化/压缩、历史、搜索、树形、深色模式/字号/自动格式化）平台无关、两端共用；只有「存储」与少数「DOM/系统」辅助函数按平台分叉。

> 构建工具为 **dx (dioxus-cli)**。已弃用旧的 `build.sh` + `dist/serve.py`。桌面构建需系统库 `webkit2gtk-4.1` + `libsoup-3.0`（Fedora：`sudo dnf install webkit2gtk4.1-devel libsoup3-devel`）。

## 常用命令

```bash
# 安装 dx CLI（一次性）；须与依赖 dioxus 版本一致
cargo install dioxus-cli --version 0.7.9 --locked

# 桌面：生成样式 → 逻辑测试 → 弹出原生窗口（脚本会先检查 webkit 依赖）
./build-desktop.sh                  # 默认 dx serve；传 build 仅构建
# 等价手动：dx serve --platform desktop

# Web：生成样式 → dx build → 逻辑测试；传 serve 本地预览
./build-web.sh                      # 默认 dx build --platform web
./build-web.sh serve                # 本地预览（dx serve --platform web）

# 运行测试
cargo test --lib --no-default-features   # 纯逻辑测试，无需 webkit（推荐日常用）
cargo test --lib                          # 同上，但默认 desktop 特性 → 需已装 webkit
cargo test test_json_formatting --no-default-features   # 单个测试（按名匹配）

# 基准测试（Criterion，纯逻辑）
cargo bench --no-default-features
```

构建前置依赖：`rustup target add wasm32-unknown-unknown`（Web）、系统 `webkit2gtk-4.1`/`libsoup-3.0`（桌面）。Tailwind 离线样式由构建脚本用 standalone CLI 自动下载并生成（见下）。

注意：`cargo build`/`test`/`bench` **默认走 desktop 特性**（`default = ["desktop"]`），在未装 webkit 的机器上会因链接 wry 失败——日常纯逻辑工作请加 `--no-default-features`。

> 已知现象（Web release 构建，dx 0.7 工具链）：`dx build --platform web --release` 末尾的 `wasm-opt` 体积优化步骤会因调试信息（DWARF）崩溃并打印 `ERROR ... wasm-opt failed`，dx 随即**回退到未优化的 wasm**——构建仍标记成功、产物可正常运行，只是体积偏大。**勿误判为构建失败**；非本项目代码问题。

## 架构（重点）

平台无关的逻辑/UI 与平台相关的实现，由两道「抽象缝」隔开——**改实现、不改签名**：

- `src/app.rs` — 整个 UI。单文件组件 `App()`，用 Dioxus signals（`use_signal`）持有全局 `AppState`，所有事件处理（格式化、历史增删改查、搜索、行号渲染）内联在此。RSX 中直接写 Tailwind class。
  - 平台相关辅助函数按 `#[cfg(target_arch = "wasm32")]` 分叉但**保持签名**：`copy_to_clipboard`、`download_text`、`sleep_ms`、文件导入 RSX 节点（Web `<input type=file>` / 桌面 `rfd` 对话框）。
  - `apply_theme` / `scroll_to_match` 统一用 `document::eval`（两端通用，构造即执行，无需 await/spawn）。样式经 `document::Stylesheet { href: asset!("/assets/tailwind.css") }` 引入。
- `src/platform/{mod,web,desktop}.rs` — **缝 1：Storage KV 垫片**。按 `cfg` 路由出同形的 `Storage::{get,set,delete}`：
  - `web.rs` 转发 `gloo_storage::LocalStorage`；`desktop.rs` 落盘到单个 `store.json`（`HashMap<String, Value>` + `Mutex` 进程内缓存，写前 `create_dir_all` + `to_string_pretty`）。
- `src/services/mod_enhanced.rs` — 全部业务逻辑与数据类型，**平台无关**。仅 `use crate::platform::Storage`：
  - `JsonService`（validate / format / minify / get_stats，基于 `serde_json`）
  - `HistoryService`（key=`eazy_json_view_history`，上限 100 条）、`ConfigService`（key=`eazy_json_view_config`）
  - 类型：`HistoryRecord`（id=时间戳，默认 name=内容的 SHA1 前 7 位短 hash）、`FormatOptions`、`ValidationResult`、`JsonStats`、`AppConfig`、`UiSettings`
  - 树形：`build_tree_rows` / `collect_container_paths` / `TreeRow`

`src/main.rs` 按 `cfg` 分支启动：wasm → `console_error_panic_hook` + `dioxus::launch`；原生 → `LaunchBuilder::desktop()` + `WindowBuilder`（标题 EazyJsonView，1200×800，最小 800×600，可调）。`src/lib.rs` 同样 `mod platform;` 并 `#[path]` 把 `mod_enhanced.rs` 暴露为 `eazy_json_view::services`，供 `benches/` 与 `src/tests.rs` 使用；单元测试挂在 lib 侧，使 `cargo test --lib --no-default-features` 不拉任何 renderer。

### 源码布局

- `main.rs` / `lib.rs` — 入口与模块挂载（cfg 分支；`#[path]` 引入 `services/mod_enhanced.rs`；`mod platform;`）
- `app.rs` — 全部 UI（含 cfg 分叉的平台辅助函数）
- `platform/{mod,web,desktop}.rs` — Storage 平台垫片
- `services/mod_enhanced.rs` — 全部业务逻辑与类型
- `tests.rs` — 单元测试
- `benches/json_performance.rs` — 基准测试
- `assets/{input.css,tailwind.css}` + `tailwind.config.js` — 离线 Tailwind（见「约定」）
- `locales/{en,zh-CN}.yml` — rust-i18n 双语译文（编译期内嵌；见「约定」的 i18n 架构缝）

改逻辑只需动 `app.rs`（界面）和 `services/mod_enhanced.rs`（逻辑/类型）；加平台差异动 `platform/`。

> 绝不同时开启 `web` 与 `desktop` 两个 renderer 特性。`[features] default = ["desktop"]` 让 dx 与 `cargo` 默认面向桌面；dx `--platform web` 会自动 `--no-default-features --features web`。

### 数据流

`app.rs` 事件 → `services::JsonService::validate` → `format` → 生成 `HistoryRecord` → `HistoryService::save_record`（经 `platform::Storage` 写 localStorage 或 store.json）→ 更新 `AppState` signal → RSX 重渲染。初始加载在 `use_effect` + `spawn` 异步读取历史与配置。

## 约定

- 代码注释与 commit message 主体为中文；**UI 文案经 rust-i18n（`locales/{en,zh-CN}.yml`，编译期内嵌）双语管理、默认英文**——运行时 `t!()` 查表 + `set_locale()` 切换，所选语言持久化到 `UiSettings.language`。输入超过 1MB 会被前端拦截提示。
- **i18n 架构缝**：`t!`/`i18n!` 只在 bin crate（`main.rs` 顶部 `i18n!("locales", fallback="en")`、`app.rs` 用 `t!`）。`services/mod_enhanced.rs` 保持**语言无关**——校验错误返回结构化的 `ValidationResult::Invalid{line,column,kind}`，翻译在 `app.rs` 按 `kind` 收敛——故 `lib` crate、benches 与 `cargo test --lib --no-default-features` 都不依赖 rust-i18n、不断言任何中文串。
  - 语言切换的响应式依赖「`App()` 为单一巨型组件」：handler 内先 `set_locale(新)` 再写 `ui_settings.language`（写信号触发整树重渲染，`t!()` 才读到新 locale）。**勿把任何调用 `t!()` 的 UI 抽成子 `#[component]`**（子组件有独立响应式 scope，不随全局 locale 重渲染）——语言切换器与 About 弹窗均内联在 `App()`。
  - locale code（`en` / `zh-CN`）须在 5 处字节一致：YAML 文件名、`i18n!(fallback=..)`、`set_locale(..)`、`UiSettings.language` 的 serde 默认、切换器按钮值。新增/改 key 时务必同步 `en.yml` 与 `zh-CN.yml`（缺 key 时 `t!` 静默回退 key 字面量，编译器不报）。
- **样式走 Tailwind（离线构建，按 prototype 设计系统）**：构建脚本用 standalone CLI 由 `assets/input.css` + `tailwind.config.js`（扫描 `src/**/*.rs` 与 `index.html`）生成 `assets/tailwind.css`，经 `app.rs` 的 `document::Stylesheet{href:asset!("/assets/tailwind.css")}` 引入（**非** CDN）。**改了 RSX 的类名或 `input.css`/`config` 后须重跑 `./tailwindcss -c tailwind.config.js -i assets/input.css -o assets/tailwind.css --minify`（或 build 脚本）才生效。**
  - **设计令牌**：prototype 配色/字体进 `tailwind.config.js` `theme.extend` 成**语义色名**（`bg-panel`/`text-ink`/`text-muted`/`border-line`/`bg-accent`/`bg-accentsoft`、语法色 `text-key|str|num|bool|null|punct`、`font-mono`=JetBrains Mono），**仅填浅色值**。
  - **深色仍由集中式 `html.dark .X { …!important }` 覆盖**（`input.css` 末段，按 prototype 深色令牌逐项重映射）；`.dark` 类机制 / `apply_theme` / `index.html` 防闪烁脚本不变。**原则：组件类（`.btn`/`.btn-secondary`/`.seg`/`.seg-item`/`.pill`/`.field`）只放几何，配色一律内联工具类**，使 `html.dark` 覆盖逐项命中。**新增/改任一语义色工具类或 `hover:` 变体后，务必在 `input.css` 补对应 `html.dark` 规则**（可用 `grep -rhoE '(hover:)?(bg|text|border)-(语义名)' src/` 机械核对覆盖完整）。
  - **prototype 新交互所在（均在 `app.rs`）**：品牌 `brand_logo()`（渐变花括号 SVG）；可折叠侧栏（`sidebar_collapsed` signal，折叠→`w-0`，主顶栏浮出 `»` 展开钮）；树行缩进引导线（每行 `depth` 个 `border-l border-guide` 占位 span）+ 折叠对象键预览（`TreeRow.collapsed_preview`）+ 计数胶囊 + hover 浮出「复制值/复制路径」（`services::node_copy_text`/`path_to_expr`，JSONPath）；底部统计胶囊栏；全窗口拖拽导入（根 div 的 `ondragover/enter/leave/drop` + `evt.files()`，需 `use dioxus::html::HasFileData`；桌面 best-effort，回退「导入」按钮）；`show_toast` 轻量反馈（替代旧 `copied` 瞬时态）。树行 hover/选中/当前匹配态用 plain CSS（`.ejv-row`/`.ejv-sel`/`.ejv-cur`/`.ejv-acts`/`.ejv-mark`/`.ejv-mark-cur`，含深色变体）。
- 修改 `serde` 序列化的类型（`HistoryRecord` 等）会影响已存 localStorage 数据的兼容性，注意字段增删。
