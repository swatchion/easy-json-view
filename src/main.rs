// EazyJsonView 应用入口（桌面 + Web 双目标）
//
// UI 与其依赖只在启用某个 renderer 特性时才编译——这样无 renderer 的兜底构建
// （如 `cargo test --no-default-features` 仍会编译 bin）不会产生大量「未使用」告警。

// 多语言初始化：无条件置于 bin crate 根，使 `_rust_i18n_translate` 始终存在（含无 renderer 的兜底
// main），app.rs 的 t!() 才能在同一 crate 内解析。fallback = "en"：缺失 key 时回退英文。
// locales/ 在编译期被内嵌，运行时仅做 HashMap 查表 + set_locale 切换，无文件 IO，wasm 可用。
rust_i18n::i18n!("locales", fallback = "en");

#[cfg(any(feature = "web", feature = "desktop"))]
mod app;
#[cfg(any(feature = "web", feature = "desktop"))]
mod platform;

#[cfg(any(feature = "web", feature = "desktop"))]
#[path = "services/mod_enhanced.rs"]
mod services;

// 入口按「目标架构 × renderer 特性」双维度门控：
//   - target_arch 决定平台代码（wasm / native）
//   - cargo feature（web / desktop）决定渲染器
// dx 构建时会成对设置二者（`--platform web` → wasm+web；`--platform desktop` → native+desktop）。
// 第三个分支兜底「无 renderer 特性」的情形（如 `cargo test --no-default-features` 仍会编译 bin），
// 只为保证任意特性组合下 bin 都能编译，不用于真正运行。

/// Web/WASM：注册 panic hook 便于浏览器控制台定位，再交给 Dioxus Web 渲染器。
#[cfg(all(target_arch = "wasm32", feature = "web"))]
fn main() {
    console_error_panic_hook::set_once();
    dioxus::launch(app::App);
}

/// 原生（桌面）：打开标题为 EazyJsonView 的原生窗口。
/// 默认 1200×800、最小 800×600、可调整大小（见 docs/requirement.md）。
#[cfg(all(not(target_arch = "wasm32"), feature = "desktop"))]
fn main() {
    use dioxus::desktop::tao::window::Icon;
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

    // 窗口图标：编译期内嵌 assets/icon.png，icon_from_memory 解码为 tao Icon。
    // image 为 dioxus-desktop 传递依赖、icon_from_memory 为其公开 helper，无需新增 Cargo 依赖。
    let icon = dioxus::desktop::icon_from_memory::<Icon>(include_bytes!("../assets/icon.png")).ok();

    let window = WindowBuilder::new()
        .with_title("EazyJsonView")
        .with_inner_size(LogicalSize::new(1200.0, 800.0))
        .with_min_inner_size(LogicalSize::new(800.0, 600.0))
        .with_window_icon(icon)
        .with_resizable(true);

    dioxus::LaunchBuilder::desktop()
        .with_cfg(Config::new().with_window(window))
        .launch(app::App);
}

/// 兜底：未启用任何 renderer 特性。仅保证 bin 可编译（例如纯逻辑/集成测试场景）。
#[cfg(not(any(
    all(target_arch = "wasm32", feature = "web"),
    all(not(target_arch = "wasm32"), feature = "desktop")
)))]
fn main() {
    eprintln!(
        "未启用任何 renderer 特性。请用 `dx serve` 或 `cargo run --features desktop|web` 运行。"
    );
}
