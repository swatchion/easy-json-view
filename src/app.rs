// 增强版 Web 应用组件
use dioxus::prelude::*;
// 拖拽导入需 HasFileData 才能在 DragData 上调用 .files()（FormData 的 files() 是固有方法，无需此 trait）。
use dioxus::html::HasFileData;
use std::collections::HashSet;
use crate::services::{JsonService, HistoryService, ValidationResult, ValidationErrorKind, FormatOptions, HistoryRecord, JsonStats, ConfigService, AppConfig, UiSettings, TreeRow, build_tree_rows, collect_container_paths, collect_search_expansions, find_matches, path_to_expr, node_copy_text};
// i18n：t!() 查表本地化文案。i18n!() 在 main.rs crate 根声明，与此处同属 bin crate。
//
// ⚠️ 语言切换的响应式机制依赖「App() 是单一组件」：set_locale() 是未被 signal 追踪的全局量，
// 单独改它不触发重渲染；切换 handler 必须同时 ui_settings.write().language（写信号）才会让
// 整个 App() 重跑、t!() 读到新 locale。因此【绝不可】把任何调用 t!() 的 UI 抽成子 #[component]
// （子组件有独立的响应式 scope，不会随全局 locale 变化重渲染）——语言切换器与 About 弹窗均内联在此。
use rust_i18n::t;

/// 结果区视图模式：文本（带语法高亮/搜索）或可折叠树形视图。作为「黏性偏好」跨文档保留。
#[derive(Clone, Copy, PartialEq)]
enum ViewMode {
    Text,
    Tree,
}

/// 启动门控三态：界面仅在 Ready（实测 Tailwind 已生效）时显示；探测期间显示加载中；
/// 探测窗口耗尽仍未生效则进入 Failed（显示「样式加载失败 · 重试」错误页，绝不揭示未样式化界面）。
#[derive(Clone, Copy, PartialEq)]
enum LoadPhase {
    Loading,
    Ready,
    Failed,
}

/// CSS 就绪探针（单次 eval 内用 rAF 轮询，`.flex` 一生效即返回 "ready"，最多约 8s 后返回 "timeout"）。
/// 建隐藏 `.flex` 探针读 computed display——无 CSS 时 div 默认 `block`，tailwind.css 生效后
/// `.flex{display:flex}` 命中变 `flex`（与主题/调色板无关的二值，不用背景色避免深浅误判）。
/// 把等待放进一次 eval，避免多次 Rust↔JS 往返各自的 poll_join 开销（实测每次往返约 1s），样式一生效即揭示。
/// 固定字面量、无任何用户输入插值。
const CSS_READY_JS: &str = "return await new Promise(res=>{const t0=performance.now();(function check(){const p=document.createElement('div');p.className='flex';p.style.cssText='position:absolute;visibility:hidden';document.body.appendChild(p);const d=getComputedStyle(p).display;p.remove();if(d==='flex')return res('ready');if(performance.now()-t0>8000)return res('timeout');requestAnimationFrame(check);})();});";

/// 树形视图最大节点数：超过则禁用「树」切换，回退文本视图以防 DOM 过大卡顿。
const TREE_NODE_CAP: usize = 3000;

/// 分栏拖动时任一面板的最小宽度（px）：仅作安全兜底，防止某一侧彻底消失、分割条无从抓回。
const SPLIT_MIN: f64 = 48.0;

/// 首次使用时可一键填入的示例 JSON
const SAMPLE_JSON: &str = r#"{"name":"Easy Json View","version":"1.0.0","tags":["json","formatter","wasm"],"active":true,"stars":42,"meta":{"author":"swatchion","license":"MIT"},"nested":{"list":[1,2,3],"empty":null}}"#;

/// 启动加载屏样式（FOUC 屏障）。**必须是 const 字面量**：写进 rsx 的 `style{}` 时 rsx 会把 `{`
/// 当插值，keyframe 花括号会编译失败——故经 `dangerous_inner_html` 注入整段。
/// 背景与 `bg-app`（浅 #e9edf3 / 深 #0a0e15，见 input.css）及 main.rs 窗口底色一致 → 揭示无色块跳变；
/// 旋转环主色用品牌蓝 #3b78ff。不含文案（避免 i18n 在 config 异步加载前显示默认英文再跳变）。
// 失败态样式也内联于此（绝不能依赖 Tailwind——它正是没加载成功才显示此页）。
const SPLASH_CSS: &str = "@keyframes ejv-spin{to{transform:rotate(360deg)}}#ejv-splash{position:fixed;inset:0;z-index:9999;display:flex;align-items:center;justify-content:center;background:#e9edf3}html.dark #ejv-splash{background:#0a0e15}.ejv-ring{width:38px;height:38px;border-radius:50%;border:3px solid rgba(59,120,255,.25);border-top-color:#3b78ff;animation:ejv-spin .7s linear infinite}.ejv-fail{display:flex;flex-direction:column;align-items:center;gap:10px;max-width:340px;padding:24px;text-align:center;font-family:system-ui,-apple-system,'Segoe UI',Roboto,sans-serif}.ejv-fail-title{font-size:15px;font-weight:600;color:#0f172a}.ejv-fail-msg{font-size:13px;line-height:1.5;color:#64748b}.ejv-retry{margin-top:4px;padding:7px 18px;font-size:13px;font-weight:500;color:#fff;background:#3b78ff;border:none;border-radius:8px;cursor:pointer}.ejv-retry:hover{background:#1f53e0}html.dark .ejv-fail-title{color:#e5e7eb}html.dark .ejv-fail-msg{color:#94a3b8}";

/// 将文本写入系统剪贴板（fire-and-forget，忽略返回的 Promise）。Web：浏览器 Clipboard API。
#[cfg(target_arch = "wasm32")]
fn copy_to_clipboard(text: &str) {
    if let Some(win) = web_sys::window() {
        let _ = win.navigator().clipboard().write_text(text);
    }
}

/// 桌面：经本应用 webview 的 `navigator.clipboard` 写入。`document::eval` 一构造即执行，无需 await。
/// 安全性：用户文本经 `serde_json::to_string` 编码为合法 JS 字符串字面量，仅作为**数据**传给
/// `writeText`，无法越出字符串上下文注入代码（非执行任意输入）。
#[cfg(not(target_arch = "wasm32"))]
fn copy_to_clipboard(text: &str) {
    let lit = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
    let _ = document::eval(&format!(
        "if (navigator.clipboard) navigator.clipboard.writeText({lit});"
    ));
}

/// 把主题应用到 `<html>` 的 class：`dark` 时加 "dark" 类，否则清空。
/// 统一走 `document::eval`（Web 与桌面通用）；调色板集中在打包 CSS 的 `html.dark { ... }` 覆盖块中。
/// eval 构造即执行，无需 await/spawn，故可在任意上下文（含 use_effect 内的异步任务）安全调用。
fn apply_theme(theme: &str) {
    let cls = if theme == "dark" { "dark" } else { "" };
    let _ = document::eval(&format!(r#"document.documentElement.className = "{cls}";"#));
}

/// 跨平台异步 sleep：Web 用 gloo-timers，桌面用 futures-timer（均运行时无关，可在 dioxus spawn 内使用）。
#[cfg(target_arch = "wasm32")]
async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}
#[cfg(not(target_arch = "wasm32"))]
async fn sleep_ms(ms: u32) {
    futures_timer::Delay::new(std::time::Duration::from_millis(ms as u64)).await;
}

/// 将已格式化的合法 JSON 文本词法切分为 (tailwind 颜色类, 文本) 序列，用于语法高亮。
/// 保留所有空白与换行，使其在 <pre> 中按原样排版。仅用于显示，不做严格校验。
/// 颜色用 prototype 语义色名（text-key/str/num/bool/null/punct），与 services::scalar_repr 一致。
fn highlight_json(s: &str) -> Vec<(&'static str, String)> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        match c {
            '"' => {
                let start = i;
                i += 1;
                while i < n {
                    match chars[i] {
                        '\\' => i += 2,
                        '"' => { i += 1; break; }
                        _ => i += 1,
                    }
                }
                let text: String = chars[start..i.min(n)].iter().collect();
                // 后面紧跟（跳过空白后）冒号 => 键，否则为字符串值
                let mut j = i;
                while j < n && chars[j].is_whitespace() { j += 1; }
                let class = if j < n && chars[j] == ':' { "text-key" } else { "text-str" };
                out.push((class, text));
            }
            '-' | '0'..='9' => {
                let start = i;
                i += 1;
                while i < n && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-')) {
                    i += 1;
                }
                out.push(("text-num", chars[start..i].iter().collect()));
            }
            't' | 'f' => {
                let start = i;
                while i < n && chars[i].is_ascii_alphabetic() { i += 1; }
                out.push(("text-bool", chars[start..i].iter().collect()));
            }
            'n' => {
                let start = i;
                while i < n && chars[i].is_ascii_alphabetic() { i += 1; }
                out.push(("text-null", chars[start..i].iter().collect()));
            }
            '{' | '}' | '[' | ']' | ':' | ',' => {
                out.push(("text-punct", c.to_string()));
                i += 1;
            }
            _ => {
                // 空白及其它字符，合并为一段无色文本
                let start = i;
                i += 1;
                while i < n
                    && !matches!(chars[i], '"' | '{' | '}' | '[' | ']' | ':' | ',' | '-' | 't' | 'f' | 'n')
                    && !chars[i].is_ascii_digit()
                {
                    i += 1;
                }
                out.push(("", chars[start..i].iter().collect()));
            }
        }
    }
    out
}

/// 结果区搜索的 `find_matches` / `MAX_SEARCH_MARKS` 已移入 `services`（文本与树形视图共用同一命中口径）。

/// 搜索高亮段：(是否匹配, 全局匹配序号, 文本)。匹配序号用于 `ejv-match-N` 锚点与「当前匹配」着色。
type SearchSeg = (bool, usize, String);

/// 把单段文本按 query 切分为高亮段；匹配段领取 `*gidx` 并自增。无匹配则整体作为一段普通文本。
fn segment_text(text: &str, query: &str, cs: bool, gidx: &mut usize) -> Vec<SearchSeg> {
    let mut segs = Vec::new();
    let mut pos = 0usize;
    for (s, e) in find_matches(text, query, cs) {
        if s > pos {
            segs.push((false, 0, text[pos..s].to_string()));
        }
        let gi = *gidx;
        *gidx += 1;
        segs.push((true, gi, text[s..e].to_string()));
        pos = e;
    }
    if pos < text.len() || segs.is_empty() {
        segs.push((false, 0, text[pos..].to_string()));
    }
    segs
}

/// 对可见树行逐行切分键/叶值高亮段，按阅读顺序（每行先键后值）分配全局匹配序号。
/// 返回与 rows 等长的 `(key_segs, val_segs)`（None 表示该部分无需搜索：无键 / 容器值）及匹配总数。
fn compute_tree_segments(
    rows: &[TreeRow],
    query: &str,
    cs: bool,
) -> (Vec<(Option<Vec<SearchSeg>>, Option<Vec<SearchSeg>>)>, usize) {
    let mut gidx = 0usize;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let key_segs = row.key_label.as_ref().map(|k| segment_text(k, query, cs, &mut gidx));
        // 容器开括号/折叠摘要、以及闭括号行均不参与搜索（与 collect_search_expansions 命中口径一致）
        let val_segs = if row.is_container || row.is_close {
            None
        } else {
            Some(segment_text(&row.value_text, query, cs, &mut gidx))
        };
        out.push((key_segs, val_segs));
    }
    (out, gidx)
}

/// 触发浏览器下载，将 text 保存为 filename（Web：Blob + 隐藏 `<a>` 下载）。
#[cfg(target_arch = "wasm32")]
fn download_text(filename: &str, text: &str) {
    use wasm_bindgen::JsCast;
    let (Some(win), ) = (web_sys::window(), ) else { return };
    let Some(doc) = win.document() else { return };
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(text));
    let Ok(blob) = web_sys::Blob::new_with_str_sequence(&parts) else { return };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else { return };
    if let Ok(el) = doc.create_element("a") {
        if let Ok(a) = el.dyn_into::<web_sys::HtmlAnchorElement>() {
            a.set_href(&url);
            a.set_download(filename);
            a.click();
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// 桌面：弹出原生「另存为」对话框，将 text 写入用户选定路径。
/// rfd 为异步，包进 `spawn`（调用点均在事件处理器内，scope 活跃）。
#[cfg(not(target_arch = "wasm32"))]
fn download_text(filename: &str, text: &str) {
    let filename = filename.to_string();
    let text = text.to_string();
    spawn(async move {
        if let Some(handle) = rfd::AsyncFileDialog::new()
            .set_file_name(&filename)
            .save_file()
            .await
        {
            let _ = std::fs::write(handle.path(), text);
        }
    });
}

/// 桌面：弹出「另存为」对话框，将二进制写入用户选定路径（mirror download_text，用于历史 zip 导出）。
/// fire-and-forget：spawn 内异步弹窗 + std::fs::write；调用点在事件处理器内，scope 活跃。
#[cfg(not(target_arch = "wasm32"))]
fn save_bytes(filename: &str, bytes: Vec<u8>) {
    let filename = filename.to_string();
    spawn(async move {
        if let Some(handle) = rfd::AsyncFileDialog::new()
            .set_file_name(&filename)
            .save_file()
            .await
        {
            let _ = std::fs::write(handle.path(), &bytes);
        }
    });
}

/// 在外部打开 URL（About 弹窗的 GitHub 链接）。两端分叉但保持签名，同 copy_to_clipboard/download_text。
/// Web：新标签打开。调用点在 onclick（用户手势）内，不触发弹窗拦截。
#[cfg(target_arch = "wasm32")]
fn open_url(url: &str) {
    if let Some(win) = web_sys::window() {
        // 带 noopener,noreferrer：window.open 不像 <a target=_blank> 那样自动断开 opener，
        // 否则新标签可经 window.opener 反向劫持本应用标签（reverse-tabnabbing）。
        let _ = win.open_with_url_and_target_and_features(url, "_blank", "noopener,noreferrer");
    }
}

/// 桌面：交给系统默认浏览器（xdg-open/open/start）。绝不能在 webview 内导航 <a href>——
/// 那会把应用页面替换成目标网页。open::that 阻塞到启动器返回，故丢到独立线程避免卡住 UI 事件循环。
#[cfg(not(target_arch = "wasm32"))]
fn open_url(url: &str) {
    let url = url.to_string();
    std::thread::spawn(move || {
        let _ = open::that(url);
    });
}

/// 项目源码仓库地址（About 弹窗「查看源码」链接）。
const GITHUB_URL: &str = "https://github.com/swatchion/easy-json-view";

/// 当前用户运行的 release 版本（编译期取自 Cargo.toml 的 version，bump 版本即自动同步）。
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 支持的界面语言：(locale code, 该语言自称 autonym)。加语言 = 加一行 + 一个 locales/<code>.yml。
/// 语言名用自称（English / 中文），按惯例不翻译——不入 YAML，故与「键集对称」检查无关。
const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("zh-CN", "中文")];

// ===== 内联线性图标（Lucide 风格） =====
// 仅用 path（不用 circle/line），把属性面收敛到已核实安全的 view_box/stroke_*/fill/d。
// 图标用 `currentColor` 继承文字色——文字色类（text-muted 等）已有 html.dark 覆盖，故深色自动适配。

/// 图标外壳：给定 size class 与若干 path 的 `d`，渲染 24×24 描边图标（继承文字色）。
fn lucide_cls(cls: &str, paths: &[&'static str]) -> Element {
    rsx! {
        svg {
            class: "{cls}",
            // 回退尺寸：CSS（assets/tailwind.css）加载前，未样式化 SVG 会按内禀 300×150 渲染
            // → 首屏闪现巨型图标（FOUC）。给 width/height=16 作回退，与 24×24 viewBox 等比缩放为 ~16px。
            // tailwind.css 加载后，class 里的 w-4 h-4 等工具类按 CSS 优先级覆盖该属性 → 最终尺寸不变。
            width: "16",
            height: "16",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            for (i, d) in paths.iter().enumerate() {
                path { key: "{i}", d: "{d}" }
            }
        }
    }
}
/// 默认 16×16 图标。
fn lucide(paths: &[&'static str]) -> Element {
    lucide_cls("w-4 h-4 shrink-0", paths)
}

/// 品牌 Logo：26×26 蓝色渐变圆角底 + 白色花括号 + 青色高亮节点（升级说明定稿）。
fn brand_logo() -> Element {
    rsx! {
        svg {
            class: "shrink-0 block",
            width: "26",
            height: "26",
            view_box: "0 0 48 48",
            fill: "none",
            defs {
                linearGradient {
                    id: "ejv-brand",
                    x1: "0", y1: "0", x2: "48", y2: "48",
                    gradient_units: "userSpaceOnUse",
                    stop { stop_color: "#3b78ff" }
                    stop { offset: "1", stop_color: "#1f53e0" }
                }
            }
            rect { width: "48", height: "48", rx: "12", fill: "url(#ejv-brand)" }
            path {
                d: "M21 14 C18 14 18 18 18 21 C18 23.5 16 24 14.5 24 C16 24 18 24.5 18 27 C18 30 18 34 21 34",
                stroke: "#fff", stroke_width: "2.6", stroke_linecap: "round", stroke_linejoin: "round",
            }
            path {
                d: "M27 14 C30 14 30 18 30 21 C30 23.5 32 24 33.5 24 C32 24 30 24.5 30 27 C30 30 30 34 27 34",
                stroke: "#fff", stroke_width: "2.6", stroke_linecap: "round", stroke_linejoin: "round",
            }
            circle { cx: "24", cy: "18.4", r: "1.7", fill: "#fff", opacity: ".85" }
            circle { cx: "24", cy: "24", r: "2.8", fill: "#22d3ee" }
            circle { cx: "24", cy: "29.6", r: "1.7", fill: "#fff", opacity: ".85" }
        }
    }
}

fn icon_globe() -> Element {
    lucide(&[
        "M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20",
        "M2 12h20",
        "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
    ])
}
fn icon_chevron_down() -> Element {
    lucide(&["m6 9 6 6 6-6"])
}
fn icon_chevron_up() -> Element {
    lucide(&["m6 15 6-6 6 6"])
}
fn icon_check() -> Element {
    lucide(&["M20 6 9 17l-5-5"])
}
fn icon_info() -> Element {
    lucide(&[
        "M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20",
        "M12 16v-4",
        "M12 8h.01",
    ])
}
fn icon_sun() -> Element {
    lucide(&[
        "M12 8a4 4 0 1 0 0 8 4 4 0 1 0 0-8",
        "M12 2v2 M12 20v2 M4.93 4.93l1.41 1.41 M17.66 17.66l1.41 1.41 M2 12h2 M20 12h2 M6.34 17.66l-1.41 1.41 M19.07 4.93l-1.41 1.41",
    ])
}
fn icon_moon() -> Element {
    lucide(&["M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9z"])
}
/// 「示例」按钮用的四角星（保留；prototype 五角星可后续单独对齐）。
fn icon_sparkles() -> Element {
    lucide(&["M12 3l1.9 5.8a2 2 0 0 0 1.3 1.3L21 12l-5.8 1.9a2 2 0 0 0-1.3 1.3L12 21l-1.9-5.8a2 2 0 0 0-1.3-1.3L3 12l5.8-1.9a2 2 0 0 0 1.3-1.3z"])
}
/// 格式化（美化）：魔棒 + 双闪（对齐 prototype）。
fn icon_format() -> Element {
    lucide(&[
        "M5 19l8-8",
        "M13 3.5l1.1 2.6L16.7 7l-2.6 1.1L13 10.7l-1.1-2.6L9.3 7l2.6-.9z",
        "M18.5 12.5l.7 1.6 1.6.7-1.6.7-.7 1.6-.7-1.6-1.6-.7 1.6-.7z",
    ])
}
/// 压缩为单行：四角向内收（对齐 prototype）。
fn icon_minify() -> Element {
    lucide(&["M4 14h6v6", "M20 10h-6V4", "M14 10l7-7", "M3 21l7-7"])
}
/// 折叠侧栏：面板 + 左竖线 + 向左收的尖角（比纯字形 « 更易识别）。
fn icon_panel_left_close() -> Element {
    lucide(&[
        "M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
        "M9 3v18",
        "m16 15-3-3 3-3",
    ])
}
/// 展开侧栏：面板 + 左竖线 + 向右展开的尖角。
fn icon_panel_left_open() -> Element {
    lucide(&[
        "M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
        "M9 3v18",
        "m14 9 3 3-3 3",
    ])
}
/// 清空：橡皮擦。
fn icon_eraser() -> Element {
    lucide(&["M7 21l-4.3-4.3c-1-1-1-2.5 0-3.4l9.6-9.6c1-1 2.5-1 3.4 0l5.6 5.6c1 1 1 2.5 0 3.4L13 21 M22 21H7 M5 11l9 9"])
}
/// 搜索：放大镜。
fn icon_search() -> Element {
    lucide(&["M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14", "m21 21-4.3-4.3"])
}
/// 复制：双层卡片。
fn icon_copy() -> Element {
    lucide(&["M11 9h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-7a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2z", "M5 15V5a2 2 0 0 1 2-2h10"])
}
/// 下载：下箭头 + 底线。
fn icon_download() -> Element {
    lucide(&["M12 3v12", "m7 11 5 5 5-5", "M5 21h14"])
}
/// 删除：垃圾桶。
fn icon_trash() -> Element {
    lucide(&["M4 7h16", "M9 7V4h6v3", "M6 7l1 13h10l1-13"])
}
/// 导入 / 拖拽：上箭头 + 底线。
fn icon_file_up() -> Element {
    lucide(&["M12 17V5", "m7 9 5-5 5 5", "M5 21h14"])
}
/// 重命名：铅笔。
fn icon_pencil() -> Element {
    lucide(&["M12 20h9", "M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z"])
}
/// 复制路径：链接。
fn icon_link() -> Element {
    lucide(&["M9 15l6-6", "M11 6l1-1a3 3 0 1 1 4 4l-1 1", "M13 18l-1 1a3 3 0 1 1-4-4l1-1"])
}

/// 应用状态
#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    /// 当前 JSON 输入内容
    pub input_content: String,
    /// 格式化后的 JSON 内容
    pub output_content: String,
    /// 是否正在处理
    pub is_processing: bool,
    /// 错误信息
    pub error_message: Option<String>,
    /// 格式化选项
    pub format_options: FormatOptions,
    /// 历史记录列表
    pub history_records: Vec<HistoryRecord>,
    /// 搜索关键词
    pub search_query: String,
    /// 当前主面板显示结果所对应的历史记录 id（用于侧栏高亮）。
    /// 格式化/选择历史时写入；手动改输入或清空时置 None（高亮消失）。
    pub current_record_id: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            input_content: String::new(),
            output_content: String::new(),
            is_processing: false,
            error_message: None,
            format_options: FormatOptions::default(),
            history_records: Vec::new(),
            search_query: String::new(),
            current_record_id: None,
        }
    }
}

#[component]
pub fn App() -> Element {
    let mut app_state = use_signal(AppState::default);
    let mut editing_record_id = use_signal(|| None::<String>);
    let mut edit_name_input = use_signal(|| String::new());
    // 最近一次成功处理的 JSON 统计信息
    let mut stats = use_signal(|| None::<JsonStats>);
    // 结果区键值搜索（与侧栏的历史搜索 search_query 区分）
    let mut output_query = use_signal(|| String::new());
    let mut search_case_sensitive = use_signal(|| false);
    let mut current_match = use_signal(|| 0usize);
    // 侧栏：只看书签
    let mut show_bookmarks_only = use_signal(|| false);
    // 结果区视图模式（文本/树），默认树形，黏性偏好（本会话内保持）
    let mut view_mode = use_signal(|| ViewMode::Tree);
    // 树形视图中已折叠的容器路径集合（按 build_tree_rows 的路径方案）
    let mut collapsed_paths = use_signal(HashSet::<String>::new);
    // UI 偏好（主题 / 字号 / 自动格式化），与 format_options 一起持久化到 config
    let mut ui_settings = use_signal(UiSettings::default);
    // 自动格式化防抖序号：每次输入自增，定时器醒来时比对，确保仅「最后一次输入」生效
    let mut autofmt_seq = use_signal(|| 0u64);
    // 功能介绍（About）弹窗的开合状态。内联在 App()（不抽子组件），故 t!() 随语言切换即时更新。
    let mut show_about = use_signal(|| false);
    // 语言切换下拉菜单的开合状态。同为内联在 App()，随语言切换即时重渲染——勿抽子组件。
    let mut show_lang_menu = use_signal(|| false);

    // ===== prototype 新增交互的会话级状态 =====
    // 树中点击高亮的节点路径（与 build_tree_rows 的位置路径一致）。
    // 用 Option 而非空串：root 行的 path 恰为 ""，空串哨兵会与之碰撞导致 root 行默认被高亮。
    let mut selected_path = use_signal(|| None::<String>);
    // 全窗口拖拽遮罩开合 + 进出深度计数（子元素进出会成对触发 enter/leave，用计数防闪烁）
    let mut drag_over = use_signal(|| false);
    let mut drag_depth = use_signal(|| 0i32);
    // 轻量 Toast：开合 + 文案 + 序号（每次显示自增，定时器醒来时比对，仅最后一次生效）
    let mut toast_open = use_signal(|| false);
    let mut toast_msg = use_signal(|| String::new());
    let mut toast_seq = use_signal(|| 0u64);
    // 可折叠侧栏（追加需求）：折叠后侧栏收为窄轨 rail（保留 logo + 展开钮 + 历史省略名）
    let mut sidebar_collapsed = use_signal(|| false);
    // 可拖拽分栏（追加需求）：input_w=None 表示默认 2:1；拖动后固定输入面板宽度（会话级，不持久化）。
    let mut input_w = use_signal(|| None::<f64>);
    // 拖动中标志：true 时根 div 的 onmousemove 据此调整 input_w
    let mut split_dragging = use_signal(|| false);
    // mousedown 时测得的主区右缘视口坐标（=输入面板右缘，贴主区右缘，拖动期间不变）
    let mut input_right = use_signal(|| 0f64);
    // mousedown 时测得的主区左缘视口坐标（=侧栏右缘）；用于把输出下限钳到 SPLIT_MIN，避免输出被挤到 0
    let mut input_left = use_signal(|| 0f64);
    // 启动门控三态：Loading 显示 spinner 遮罩；探测确认 Tailwind 生效→Ready 揭示界面；
    // 探测窗口耗尽仍未生效→Failed 显示错误页。绝不在样式生效前揭示界面（无提前兜底）。
    // 「重试」走整页 reload（见错误页按钮）：失败的样式表 <link> 不会自动重取，只重跑探针无效。
    let mut load_phase = use_signal(|| LoadPhase::Loading);

    // 语言切换的响应式保险：显式读取 language，使 App() 订阅该字段——切换器写入时触发整树重渲染。
    let _lang = ui_settings.read().language.clone();

    // 初始化时加载历史记录与持久化配置
    use_effect(move || {
        spawn(async move {
            if let Ok(records) = HistoryService::load_history().await {
                app_state.write().history_records = records;
            }
            if let Ok(cfg) = ConfigService::load_config().await {
                app_state.write().format_options = cfg.format_options;
                apply_theme(&cfg.ui_settings.theme);
                rust_i18n::set_locale(&cfg.ui_settings.language);
                ui_settings.set(cfg.ui_settings);
            }
        });
    });

    // 启动门控的揭示逻辑（独立于上面的读盘 effect）：无响应式依赖，挂载后只跑一次（「重试」走整页 reload 重新挂载）。
    use_effect(move || {
        // 移除 index.html 泄漏的静态启动屏：dx 0.7 把应用「追加」进 #main 而非替换，故静态 #ejv-boot
        // 会永久浮在最上层盖住应用。Dioxus 自身的 splash 自首帧起即接管覆盖，此处移除静态屏即无缝衔接。
        // eval 构造即执行，无需 await。
        let _ = document::eval("document.getElementById('ejv-boot')?.remove();");
        // 检测：单次 eval 内用 rAF 轮询，`.flex` 一生效立刻 resolve('ready')，最多约 8s 后 'timeout'。
        // eval 的 await-返回值形式与分栏拖动测量同款。安全：CSS_READY_JS 为固定字面量、无用户输入插值。
        spawn(async move {
            let ready = matches!(document::eval(CSS_READY_JS).await, Ok(v) if v.as_str() == Some("ready"));
            // 'ready' → 揭示；'timeout' 或 eval 失败 → 错误页（绝不揭示未样式化界面）。
            load_phase.set(if ready { LoadPhase::Ready } else { LoadPhase::Failed });
        });
    });

    // 轻量 Toast：1.5s 自动消失（序号防抖，新 toast 顶替旧定时器）
    let mut show_toast = move |msg: String| {
        *toast_seq.write() += 1;
        let seq = *toast_seq.read();
        toast_msg.set(msg);
        toast_open.set(true);
        spawn(async move {
            sleep_ms(1500).await;
            if *toast_seq.read() == seq {
                toast_open.set(false);
            }
        });
    };

    // 处理函数：minify=false 为格式化（美化），minify=true 为压缩为单行
    let mut process = move |minify: bool| {
        // trim 单一收口：处理与存储均用 trim 后的值（textarea 内容不变，用户仍见原输入）。
        // 这样首尾空白不同但 JSON 相同的输入会命中同一条历史记录（去重键 = trim 后 content）。
        let input = app_state.read().input_content.trim().to_string();
        let options = app_state.read().format_options.clone();

        if input.is_empty() {
            app_state.write().error_message = Some(t!("err.input_required").to_string());
            return;
        }

        // 安全上限：单行渲染后大文件已不再卡顿，仅对超大输入（>10MB）作硬拦截
        if input.len() > 10_000_000 {
            let mb = format!("{:.1}", input.len() as f64 / 1_048_576.0);
            app_state.write().error_message =
                Some(t!("err.too_large", mb = mb).to_string());
            return;
        }

        app_state.write().is_processing = true;
        app_state.write().error_message = None;

        spawn(async move {
            match JsonService::validate(&input) {
                ValidationResult::Valid => {
                    let result = if minify {
                        JsonService::minify(&input)
                    } else {
                        JsonService::format(&input, &options)
                    };
                    match result {
                        Ok(output) => {
                            // 去重保留：先在内存历史中按 trim 后 content 查重。提取 owned 元组后
                            // 立即释放 read 守卫（不跨 .await 持有 RefCell 借用，避免 panic）。
                            let existing = app_state.read().history_records.iter()
                                .find(|r| r.content == input)
                                .map(|r| (r.id.clone(), r.formatted_content.clone()));
                            match existing {
                                // 命中已有记录：仅高亮它，不新增、不置顶、不动 id/时间/书签。
                                // 若本次缩进选项变化致输出不同，则就地更新其 formatted_content（内存 + 持久化）。
                                Some((id, old_formatted)) => {
                                    if old_formatted != output
                                        && HistoryService::update_record_formatted(&id, output.clone()).await.is_ok()
                                    {
                                        if let Some(r) = app_state.write().history_records.iter_mut().find(|r| r.id == id) {
                                            r.formatted_content = output.clone();
                                        }
                                    }
                                    app_state.write().current_record_id = Some(id);
                                }
                                // 未命中：新建记录、保存、重载，并高亮新记录。
                                None => {
                                    let record = HistoryRecord::new(input.clone(), output.clone());
                                    let new_id = record.id.clone();
                                    if HistoryService::save_record(&record).await.is_ok() {
                                        if let Ok(records) = HistoryService::load_history().await {
                                            app_state.write().history_records = records;
                                        }
                                    }
                                    app_state.write().current_record_id = Some(new_id);
                                }
                            }
                            stats.set(JsonService::get_stats(&input).ok());
                            app_state.write().output_content = output;
                            current_match.set(0);
                            collapsed_paths.set(HashSet::new());
                            selected_path.set(None);
                        }
                        Err(e) => {
                            app_state.write().error_message = Some(t!("err.process_failed", msg = e.to_string()).to_string());
                        }
                    }
                }
                // 翻译收敛点：service 只给出结构化的 line/column/kind，在此按 kind 用 t!() 本地化。
                ValidationResult::Invalid { line, column, kind } => {
                    let msg = match kind {
                        ValidationErrorKind::Empty => t!("err.empty").to_string(),
                        ValidationErrorKind::Incomplete => {
                            t!("err.incomplete", line = line, col = column).to_string()
                        }
                        ValidationErrorKind::Syntax(detail) => {
                            let reason = detail.split(" at line ").next().unwrap_or(&detail);
                            t!("err.syntax", line = line, col = column, msg = reason).to_string()
                        }
                    };
                    app_state.write().error_message = Some(msg);
                }
            }
            app_state.write().is_processing = false;
        });
    };

    // 复制到剪贴板 + Toast 反馈
    let mut do_copy = move |text: String| {
        copy_to_clipboard(&text);
        show_toast(t!("btn.copied").to_string());
    };

    // 清空输入与输出（不影响历史记录）
    let clear_all = move |_| {
        let mut state = app_state.write();
        state.input_content = String::new();
        state.output_content = String::new();
        state.error_message = None;
        state.current_record_id = None; // 清空 → 无「当前」记录，高亮消失
        drop(state);
        stats.set(None);
        output_query.set(String::new());
        current_match.set(0);
        collapsed_paths.set(HashSet::new());
        selected_path.set(None);
    };

    // 持久化「当前两份偏好」（格式化选项 + UI 设置）
    let save_cfg = move || {
        let cfg = AppConfig {
            format_options: app_state.read().format_options.clone(),
            ui_settings: ui_settings.read().clone(),
        };
        spawn(async move {
            let _ = ConfigService::save_config(&cfg).await;
        });
    };

    // 软格式化（自动格式化路径）：校验通过则就地刷新输出/统计，但【不写历史】、非法 JSON 静默返回。
    let mut auto_format_now = move || {
        let input = app_state.read().input_content.clone();
        let options = app_state.read().format_options.clone();
        if input.trim().is_empty() || input.len() > 10_000_000 {
            return;
        }
        if !matches!(JsonService::validate(&input), ValidationResult::Valid) {
            return;
        }
        if let Ok(output) = JsonService::format(&input, &options) {
            stats.set(JsonService::get_stats(&input).ok());
            app_state.write().output_content = output;
            app_state.write().error_message = None;
            current_match.set(0);
            collapsed_paths.set(HashSet::new());
            selected_path.set(None);
        }
    };

    // 填入示例 JSON 并立即格式化
    let load_sample = move |_| {
        app_state.write().input_content = SAMPLE_JSON.to_string();
        process(false);
    };

    // 历史记录选择处理
    let mut handle_history_select = move |record: HistoryRecord| {
        stats.set(JsonService::get_stats(&record.content).ok());
        // 加载历史记录即「当前」记录 → 高亮该行（取 id 须在下面移动 content 之前）
        app_state.write().current_record_id = Some(record.id.clone());
        app_state.write().input_content = record.content;
        app_state.write().output_content = record.formatted_content;
        current_match.set(0);
        collapsed_paths.set(HashSet::new());
        selected_path.set(None);
    };

    // 切换书签：经服务层持久化后，同步翻转内存中对应记录的标志
    let handle_toggle_bookmark = move |record_id: String| {
        spawn(async move {
            if let Ok(new_state) = HistoryService::toggle_bookmark(&record_id).await {
                if let Some(r) = app_state.write().history_records.iter_mut().find(|r| r.id == record_id) {
                    r.bookmarked = new_state;
                }
            }
        });
    };

    // 结果区搜索：滚动到指定序号的匹配项（document::eval，Web 与桌面通用）
    let scroll_to_match = move |idx: usize| {
        let _ = document::eval(&format!(
            "var el=document.getElementById('ejv-match-{idx}'); if(el) el.scrollIntoView({{block:'center'}});"
        ));
    };

    // 删除历史记录
    let handle_history_delete = move |record_id: String| {
        spawn(async move {
            if let Ok(_) = HistoryService::delete_record(&record_id).await {
                app_state.write().history_records.retain(|r| r.id != record_id);
            }
        });
    };

    // 修改历史记录名称
    let handle_edit_history_name = move |(record_id, new_name): (String, String)| {
        spawn(async move {
            if let Ok(_) = HistoryService::update_record_name(&record_id, new_name.clone()).await {
                if let Some(record) = app_state.write().history_records.iter_mut().find(|r| r.id == record_id) {
                    record.name = new_name;
                }
            }
        });
    };

    // 处理编辑按钮点击
    let mut handle_edit_click = move |record: HistoryRecord| {
        editing_record_id.set(Some(record.id.clone()));
        edit_name_input.set(record.name.clone());
    };

    // 处理编辑确认
    let mut handle_edit_confirm = move |record_id: String| {
        let new_name = edit_name_input.read().clone();
        if !new_name.trim().is_empty() {
            handle_edit_history_name((record_id, new_name));
        }
        editing_record_id.set(None);
        edit_name_input.set(String::new());
    };

    // 处理编辑取消
    let mut handle_edit_cancel = move |_| {
        editing_record_id.set(None);
        edit_name_input.set(String::new());
    };

    // 过滤历史记录：按搜索词 +「只看书签」过滤，并将书签置顶（稳定排序保留各组内的时间序）
    let filtered_records = {
        let query = app_state.read().search_query.to_lowercase();
        let only_bm = *show_bookmarks_only.read();
        let mut list: Vec<HistoryRecord> = app_state.read().history_records.iter()
            .filter(|record| {
                (!only_bm || record.bookmarked)
                    && (query.is_empty()
                        || record.name.to_lowercase().contains(&query)
                        || record.content.to_lowercase().contains(&query))
            })
            .cloned()
            .collect();
        list.sort_by_key(|r| !r.bookmarked);
        list
    };

    // 当前主面板结果所对应的历史记录 id（侧栏两处历史行据此高亮；在此读一次，订阅该字段）
    let current_id = app_state.read().current_record_id.clone();

    // 输入区状态栏文案（仅字符数；行数指示按用户要求移除）
    let input_stats = t!("input.stats",
        chars = app_state.read().input_content.len()
    ).to_string();

    // 代码区字号：仅作用于输入框/输出/树（不动整体布局字号）
    let code_font = format!("font-size: {}px;", ui_settings.read().font_size);
    // 显示密度 → 格式化结果区行距（紧凑收紧，便于一屏看更多；输入 textarea 不受影响，仍 1.7）
    let code_lh = if ui_settings.read().density == "compact" { "1.25" } else { "1.7" };
    // 树形缩进引导线宽度随缩进大小缩放（2/4/8 → 8/16/32px；4=默认≈16px，渲染期计算故改缩进即时反映）
    let tree_indent_px = app_state.read().format_options.indent_size * 4;
    // 文本逐行视图每层缩进字符数（= 缩进大小，按 ch 占位与格式化文本对齐）
    let indent_size = app_state.read().format_options.indent_size;

    // 当前语言自称（globe 触发器上显示）
    let cur_lang = ui_settings.read().language.clone();
    let cur_lang_name: String = LANGUAGES
        .iter()
        .find(|pair| pair.0 == cur_lang.as_str())
        .map(|pair| pair.1.to_string())
        .unwrap_or(cur_lang.clone());

    // 预计算输出与状态标志
    let formatted_output = app_state.read().output_content.clone();
    let has_output = !formatted_output.is_empty();
    let has_error = app_state.read().error_message.is_some();
    let is_processing = app_state.read().is_processing;
    let output_line_count = if formatted_output.is_empty() { 0 } else { formatted_output.lines().count() };
    // 文本视图行号串：仅在开启且有输出时构建（单个字符串 "1\n2\n…\nN"，gutter 一个 <pre> 元素渲染，
    // DOM 量与行数无关）。关闭时为空串、不渲染 gutter。
    let line_numbers_str = if ui_settings.read().show_line_numbers && output_line_count > 0 {
        // 按最大行号位数左侧补空格（gutter <pre> 为 whitespace-pre + 等宽字体，前导空格保留并等宽对齐）
        let w = output_line_count.to_string().len().max(2);
        (1..=output_line_count).map(|n| format!("{:>w$}", n, w = w)).collect::<Vec<_>>().join("\n")
    } else {
        String::new()
    };
    // 语法高亮：仅在中小规模输出时启用，超大输出回退为纯文本以保持性能
    let highlight_tokens: Option<Vec<(&'static str, String)>> =
        if has_output && output_line_count <= 1000 && formatted_output.len() <= 100_000 {
            Some(highlight_json(&formatted_output))
        } else {
            None
        };

    // 结果区键值搜索：计算匹配并切分为渲染分段（查询非空时优先于语法高亮）
    let output_query_str = output_query.read().clone();
    let search_active = !output_query_str.is_empty();
    let search_matches = if search_active {
        find_matches(&formatted_output, &output_query_str, *search_case_sensitive.read())
    } else {
        Vec::new()
    };
    let match_count = search_matches.len();
    let search_segments: Vec<(bool, usize, String)> = if !search_active {
        Vec::new()
    } else {
        let mut segs = Vec::new();
        let mut pos = 0usize;
        for (mi, (s, e)) in search_matches.iter().enumerate() {
            if *s > pos {
                segs.push((false, 0, formatted_output[pos..*s].to_string()));
            }
            segs.push((true, mi, formatted_output[*s..*e].to_string()));
            pos = *e;
        }
        if pos < formatted_output.len() {
            segs.push((false, 0, formatted_output[pos..].to_string()));
        }
        segs
    };

    // 树形视图预计算（仅在树模式执行，文本模式零开销）
    let in_tree_mode = *view_mode.read() == ViewMode::Tree;
    let tree_node_count: Option<usize> = stats.read().as_ref().map(|s| s.total_values());
    let tree_over_cap = tree_node_count.map_or(false, |n| n > TREE_NODE_CAP);
    let tree_search_cs = *search_case_sensitive.read();
    let (tree_rows, tree_seg_rows, tree_match_count): (
        Option<Vec<TreeRow>>,
        Vec<(Option<Vec<SearchSeg>>, Option<Vec<SearchSeg>>)>,
        usize,
    ) = if in_tree_mode && has_output && !tree_over_cap {
        match serde_json::from_str::<serde_json::Value>(&formatted_output) {
            Ok(v) => {
                let effective: HashSet<String> = if search_active {
                    let needed = collect_search_expansions(&v, &output_query_str, tree_search_cs);
                    collapsed_paths.read().difference(&needed).cloned().collect()
                } else {
                    collapsed_paths.read().clone()
                };
                let rows = build_tree_rows(&v, &effective, TREE_NODE_CAP);
                let (segs, count) = if search_active {
                    compute_tree_segments(&rows, &output_query_str, tree_search_cs)
                } else {
                    (Vec::new(), 0)
                };
                (Some(rows), segs, count)
            }
            Err(_) => (None, Vec::new(), 0),
        }
    } else {
        (None, Vec::new(), 0)
    };

    let tree_render_active = tree_rows.is_some();
    // 视图切换按钮的「激活」态应反映实际渲染：树模式但超节点上限时回退文本渲染，
    // 故此时 Text 显激活、Tree 显未激活（且禁用），避免「高亮的是 Tree 却显示文本」的错位。
    let showing_text = !in_tree_mode || tree_over_cap;

    // 文本视图逐行预计算（与树形并行、互斥执行）：复用 build_tree_rows（不折叠 => 全展开、
    // 各行带 path）使文本视图也能 hover 复制值/路径 + 括号匹配高亮。行数超上限回退纯 <pre>。
    // 行数与 build_tree_rows 行数 1:1（已验证），故用 output_line_count 判上限。
    let text_over_cap = output_line_count > TREE_NODE_CAP;
    let (text_rows, text_seg_rows, text_match_count): (
        Option<Vec<TreeRow>>,
        Vec<(Option<Vec<SearchSeg>>, Option<Vec<SearchSeg>>)>,
        usize,
    ) = if showing_text && has_output && !text_over_cap {
        match serde_json::from_str::<serde_json::Value>(&formatted_output) {
            Ok(v) => {
                // 文本视图不折叠：空集 => 全部展开（含闭括号行），逐行对应格式化 JSON 文本
                let rows = build_tree_rows(&v, &HashSet::new(), TREE_NODE_CAP);
                let (segs, count) = if search_active {
                    compute_tree_segments(&rows, &output_query_str, tree_search_cs)
                } else {
                    (Vec::new(), 0)
                };
                (Some(rows), segs, count)
            }
            Err(_) => (None, Vec::new(), 0),
        }
    } else {
        (None, Vec::new(), 0)
    };
    let text_render_active = text_rows.is_some();

    // 匹配计数按当前实际渲染：树行 / 文本逐行 / 超大回退纯 <pre>（find_matches）。
    let active_match_count = if tree_render_active {
        tree_match_count
    } else if text_render_active {
        text_match_count
    } else {
        match_count
    };
    let cur_match = if active_match_count == 0 { 0 } else { (*current_match.read()).min(active_match_count - 1) };
    let cur_match_display = if active_match_count == 0 { 0 } else { cur_match + 1 };

    // 结果区搜索：上一项/下一项（delta=+1/-1），循环并滚动到位（计数按当前视图模式）
    let mut go_match = move |delta: i32| {
        let count = active_match_count;
        if count == 0 {
            return;
        }
        // 从 UI 实际显示的（已 clamp 的）cur_match 起步，避免视图/查询切换后 current_match 残留高位
        // 导致首次「上/下一个」从陈旧索引计算而出现一次空点击。
        let cur = cur_match as i32;
        let next = (((cur + delta) % count as i32) + count as i32) % count as i32;
        current_match.set(next as usize);
        scroll_to_match(next as usize);
    };

    // 底部统计胶囊：标签（含计数）+ 带色点
    let stat_pills: Vec<(String, &'static str)> = match stats.read().as_ref() {
        Some(s) => vec![
            (t!("stats.objects", n = s.objects).to_string(), "bg-muted2"),
            (t!("stats.arrays", n = s.arrays).to_string(), "bg-muted2"),
            (t!("stats.keys", n = s.keys).to_string(), "bg-key"),
            (t!("stats.strings", n = s.strings).to_string(), "bg-str"),
            (t!("stats.numbers", n = s.numbers).to_string(), "bg-num"),
            (t!("stats.booleans", n = s.booleans).to_string(), "bg-bool"),
            (t!("stats.nulls", n = s.nulls).to_string(), "bg-null"),
        ],
        None => Vec::new(),
    };
    let total_label = stats.read().as_ref().map(|s| t!("stats.total", n = s.total_values()).to_string());

    rsx! {
        // 打包后的 Tailwind 静态样式（离线，Web 与桌面共用；asset! 在编译期处理路径与指纹）
        document::Stylesheet { href: asset!("/assets/tailwind.css") }

        // ===== 根容器（全窗口 flex；挂全局拖拽监听 + 分栏拖动监听）=====
        div {
            class: "flex h-screen overflow-hidden bg-app text-ink",
            // 分栏拖动期间禁用选区/统一光标，避免拖动时选中面板文字。
            // 非拖动分支须显式写回两个属性的默认值（而非空串）：Dioxus 0.7 在 style 值变为 ""
            // 时不会移除已设的内联属性，会把 cursor:col-resize / user-select:none 残留下来 → 鼠标卡住。
            style: if *split_dragging.read() { "user-select:none;cursor:col-resize;" } else { "user-select:auto;cursor:auto;" },
            // 分栏拖动：在根 div 上监听 move/up，保证指针移出分割条仍能持续调整
            onmousemove: move |e| {
                if *split_dragging.read() {
                    let right = *input_right.read();
                    let left = *input_left.read();
                    if right > 0.0 {
                        let x = e.client_coordinates().x;
                        // 用户可自由拖动：输入/输出各留 SPLIT_MIN(48px) 兜底，防止任一面板彻底消失。
                        // 上界用主区可用宽(right-left)减 SPLIT_MIN（而非 right-SPLIT_MIN），否则忽略侧栏宽度会让输出被挤到 0。
                        // `.max(SPLIT_MIN)` 保证上界≥下界，clamp 永不 min>max。
                        let max_w = (right - left - SPLIT_MIN).max(SPLIT_MIN);
                        input_w.set(Some((right - x).clamp(SPLIT_MIN, max_w)));
                    }
                }
            },
            onmouseup: move |_| split_dragging.set(false),
            // 指针移出窗口（在窗口外释放鼠标时根 onmouseup 不触发）即结束拖动，避免 cursor 卡在 col-resize。
            onmouseleave: move |_| split_dragging.set(false),
            // 格式化快捷键（全局）：keydown 冒泡到根 div，焦点在 textarea / 按钮 / 任意 app 内元素均生效。
            // Ctrl 或 ⌘(meta)+Enter 触发；handler 只此一处（勿同时挂在 textarea，否则同一事件冒泡触发两次）。
            onkeydown: move |event| {
                if (event.data().modifiers().ctrl() || event.data().modifiers().meta()) && event.data().key() == Key::Enter {
                    process(false);
                }
            },
            // 拖拽导入：ondragover 必须 preventDefault 才允许 drop；enter/leave 用深度计数控制遮罩。
            // Web 经 evt.files() 读取首个文件；桌面（wry）best-effort（不行则用「导入」按钮，遮罩仍在）。
            ondragover: move |evt| { evt.prevent_default(); },
            ondragenter: move |evt| {
                evt.prevent_default();
                *drag_depth.write() += 1;
                drag_over.set(true);
            },
            ondragleave: move |_evt| {
                let d = { let mut w = drag_depth.write(); *w -= 1; *w };
                if d <= 0 {
                    drag_depth.set(0);
                    drag_over.set(false);
                }
            },
            ondrop: move |evt| {
                evt.prevent_default();
                drag_depth.set(0);
                drag_over.set(false);
                if let Some(file) = evt.files().into_iter().next() {
                    spawn(async move {
                        if let Ok(contents) = file.read_string().await {
                            app_state.write().input_content = contents;
                            process(false);
                            show_toast(t!("toast.imported").to_string());
                        }
                    });
                }
            },

            // ===== 左侧边栏（可折叠）=====
            aside {
                class: if *sidebar_collapsed.read() {
                    "w-16 shrink-0 overflow-hidden border-r border-line bg-panel flex flex-col transition-all duration-200"
                } else {
                    "w-[300px] shrink-0 overflow-hidden border-r border-line bg-panel flex flex-col transition-all duration-200"
                },

                if *sidebar_collapsed.read() {
                    // ===== 折叠态：窄轨 rail（保品牌可见 + 醒目展开钮 + 历史省略名导航）=====
                    div {
                        class: "pt-4 pb-3 w-full flex flex-col items-center gap-2 border-b border-line",
                        {brand_logo()}
                        button {
                            class: "w-8 h-8 grid place-items-center rounded-md text-muted hover:text-ink hover:bg-field transition-colors",
                            title: t!("sidebar.expand").to_string(),
                            "aria-label": t!("sidebar.expand").to_string(),
                            onclick: move |_| sidebar_collapsed.set(false),
                            {icon_panel_left_open()}
                        }
                    }
                    div {
                        class: "ejv-scroll flex-1 min-h-0 w-full overflow-y-auto py-2 px-1.5 flex flex-col gap-1",
                        for record in filtered_records.iter() {
                            {
                                let r = record.clone();
                                let name = record.name.clone();
                                // 当前记录 → 复用 .ejv-cur（左强调条 + 底色，浅/深色已在 input.css 定义）
                                let cur_cls = if current_id.as_deref() == Some(record.id.as_str()) { "ejv-cur" } else { "" };
                                rsx! {
                                    button {
                                        key: "{record.id}",
                                        class: "ejv-row w-full px-1 py-1.5 rounded-md text-[10px] font-mono text-muted hover:text-ink truncate text-center {cur_cls}",
                                        title: "{name}",
                                        onclick: move |_| handle_history_select(r.clone()),
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
                } else {
                // 头部：品牌 + 语言菜单 + 折叠按钮
                div {
                    class: "p-4 border-b border-line",
                    div {
                        class: "flex items-center gap-1.5",
                        {brand_logo()}
                        span {
                            class: "text-[15px] font-extrabold tracking-tight text-ink truncate min-w-0",
                            "Easy "
                            span { class: "text-accent", "Json View" }
                        }
                        div { class: "flex-1" }

                        // 语言切换：globe + 当前语言 + ▾（紧凑，确保 info/折叠钮在 300px 侧栏内不被裁切）
                        div {
                            class: "relative shrink-0",
                            button {
                                class: "flex items-center gap-1 text-[11px] text-ink bg-field border border-fieldline rounded-lg px-1.5 py-1 hover:border-muted2 transition-colors",
                                title: t!("lang.menu_aria").to_string(),
                                "aria-label": t!("lang.menu_aria").to_string(),
                                onclick: move |_| {
                                    let v = *show_lang_menu.read();
                                    show_lang_menu.set(!v);
                                },
                                {icon_globe()}
                                span { class: "whitespace-nowrap", "{cur_lang_name}" }
                                // 菜单展开时箭头翻转，给出开合反馈
                                if *show_lang_menu.read() { {icon_chevron_up()} } else { {icon_chevron_down()} }
                            }
                            if *show_lang_menu.read() {
                                div {
                                    class: "fixed inset-0 z-40",
                                    onclick: move |_| show_lang_menu.set(false),
                                }
                                div {
                                    class: "absolute right-0 mt-2 z-50 min-w-[8rem] rounded-xl bg-panel border border-line shadow-panel p-1",
                                    for lang in LANGUAGES {
                                        {
                                            let code = lang.0;
                                            let name = lang.1;
                                            let active = code == cur_lang.as_str();
                                            rsx! {
                                                button {
                                                    key: "{code}",
                                                    class: if active {
                                                        "w-full flex items-center justify-between gap-2 px-3 py-2 text-xs text-left text-accent font-semibold rounded-md hover:bg-field"
                                                    } else {
                                                        "w-full flex items-center justify-between gap-2 px-3 py-2 text-xs text-left text-ink rounded-md hover:bg-field"
                                                    },
                                                    onclick: move |_| {
                                                        rust_i18n::set_locale(code);
                                                        ui_settings.write().language = code.to_string();
                                                        save_cfg();
                                                        show_lang_menu.set(false);
                                                    },
                                                    span { "{name}" }
                                                    if active { {icon_check()} }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 折叠侧栏（醒目图标 + 放大点击区）
                        button {
                            class: "w-8 h-8 shrink-0 grid place-items-center rounded-md text-muted hover:text-ink hover:bg-field transition-colors",
                            title: t!("sidebar.collapse").to_string(),
                            "aria-label": t!("sidebar.collapse").to_string(),
                            onclick: move |_| sidebar_collapsed.set(true),
                            {icon_panel_left_close()}
                        }
                    }
                    // 副标题行：左副标题 + 右 About ⓘ（同一行对齐）
                    div {
                        class: "flex items-center justify-between gap-2 mt-1.5",
                        p {
                            class: "text-[11.5px] text-muted min-w-0 truncate",
                            {t!("app.subtitle").to_string()}
                        }
                        button {
                            class: "w-8 h-8 shrink-0 grid place-items-center rounded-full border border-line text-muted hover:text-ink hover:border-muted2 transition-colors",
                            title: t!("about.button").to_string(),
                            "aria-label": t!("about.button").to_string(),
                            onclick: move |_| show_about.set(true),
                            {icon_info()}
                        }
                    }
                }

                // 搜索 + 只看书签 + 清空
                div {
                    class: "px-4 py-3 border-b border-line2 flex flex-col gap-3",
                    div {
                        class: "relative",
                        span {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted2 flex pointer-events-none",
                            {icon_search()}
                        }
                        input {
                            class: "field w-full bg-field border-fieldline text-ink text-xs pl-9 pr-3 py-2",
                            placeholder: t!("search.history_placeholder").to_string(),
                            r#type: "text",
                            value: app_state.read().search_query.clone(),
                            oninput: move |event| {
                                app_state.write().search_query = event.value();
                            }
                        }
                    }
                    div {
                        class: "flex items-center justify-between gap-2",
                        label {
                            class: "flex items-center gap-1.5 text-xs text-muted cursor-pointer select-none min-w-0",
                            title: t!("history.only_bookmarks_title").to_string(),
                            input {
                                r#type: "checkbox",
                                class: "w-3.5 h-3.5 cursor-pointer",
                                checked: *show_bookmarks_only.read(),
                                onchange: move |event| show_bookmarks_only.set(event.checked()),
                            }
                            span { class: "text-[#e8a200]", "★" }
                            span { class: "truncate", {t!("history.only_bookmarks").to_string()} }
                        }
                        div {
                            class: "flex items-center gap-1 shrink-0",
                            // 历史 zip 导出/导入（仅桌面，作用于【历史记录】）——与输入区「导入到输入框」分开。
                            // Web 端不出现这两钮（zip 依赖被 cfg 排除）。
                            {
                                #[cfg(not(target_arch = "wasm32"))]
                                let zip_actions = rsx! {
                                    if !app_state.read().history_records.is_empty() {
                                        button {
                                            class: "p-1 rounded text-muted hover:text-ink hover:bg-field transition-colors",
                                            title: t!("btn.export_zip_title").to_string(),
                                            "aria-label": t!("btn.export_zip").to_string(),
                                            onclick: move |_| {
                                                let records = app_state.read().history_records.clone();
                                                match crate::services::export_zip(&records) {
                                                    Ok(bytes) => {
                                                        let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
                                                        save_bytes(&format!("easy-json-view-history-{ts}.zip"), bytes);
                                                        show_toast(t!("toast.exported").to_string());
                                                    }
                                                    Err(_) => show_toast(t!("toast.export_failed").to_string()),
                                                }
                                            },
                                            {icon_download()}
                                        }
                                    }
                                    button {
                                        class: "p-1 rounded text-muted hover:text-ink hover:bg-field transition-colors",
                                        title: t!("btn.import_zip_title").to_string(),
                                        "aria-label": t!("btn.import_zip").to_string(),
                                        onclick: move |_| {
                                            spawn(async move {
                                                if let Some(handle) = rfd::AsyncFileDialog::new()
                                                    .add_filter("Zip", &["zip"])
                                                    .pick_file()
                                                    .await
                                                {
                                                    let bytes = handle.read().await;
                                                    match crate::services::parse_zip(&bytes) {
                                                        Ok(records) => match HistoryService::merge_records(records).await {
                                                            Ok(added) => {
                                                                if let Ok(all) = HistoryService::load_history().await {
                                                                    app_state.write().history_records = all;
                                                                }
                                                                show_toast(t!("toast.merged", n = added).to_string());
                                                            }
                                                            Err(_) => show_toast(t!("toast.import_invalid").to_string()),
                                                        },
                                                        Err(_) => show_toast(t!("toast.import_invalid").to_string()),
                                                    }
                                                }
                                            });
                                        },
                                        {icon_file_up()}
                                    }
                                };
                                #[cfg(target_arch = "wasm32")]
                                let zip_actions = rsx! {};
                                zip_actions
                            }
                            if !app_state.read().history_records.is_empty() {
                                button {
                                    class: "inline-flex items-center gap-1.5 text-[11.5px] text-danger px-1.5 py-1 rounded-md hover:bg-field transition-colors",
                                    onclick: move |_| {
                                        spawn(async move {
                                            if let Ok(_) = HistoryService::clear_history().await {
                                                app_state.write().history_records.clear();
                                            }
                                        });
                                    },
                                    {icon_trash()}
                                    {t!("history.clear").to_string()}
                                }
                            }
                        }
                    }
                }

                // 历史记录列表
                div {
                    class: "ejv-scroll flex-1 min-h-0 overflow-y-auto px-3 py-3 flex flex-col gap-2",
                    if filtered_records.is_empty() {
                        {
                            let (glyph, label) = if *show_bookmarks_only.read() {
                                ("☆", t!("history.empty_no_bookmarks").to_string())
                            } else if app_state.read().search_query.is_empty() {
                                ("☰", t!("history.empty_none").to_string())
                            } else {
                                ("⌕", t!("history.empty_no_match").to_string())
                            };
                            rsx! {
                                div {
                                    class: "p-8 flex flex-col items-center justify-center text-center text-muted2 select-none",
                                    div { class: "text-3xl mb-2", "{glyph}" }
                                    p { class: "text-xs", "{label}" }
                                }
                            }
                        }
                    } else {
                        for record in filtered_records.iter() {
                            {
                                let record_clone = record.clone();
                                let record_name = record.name.clone();
                                let record_id = record.id.clone();
                                // 当前记录高亮：用条件工具类（border-accent + bg-accentsoft，与 .ejv-cur 同底色），
                                // 而非 .ejv-cur——卡片自带 bg-panel2，深色 html.dark .bg-panel2{!important} 会压过 .ejv-cur 底色。
                                // 两串仅在 border/bg 令牌处不同。
                                let card_cls = if current_id.as_deref() == Some(record_id.as_str()) {
                                    "group border border-accent bg-accentsoft rounded-xl p-3 transition-colors hover:border-accent hover:bg-field"
                                } else {
                                    "group border border-line2 bg-panel2 rounded-xl p-3 transition-colors hover:border-accent hover:bg-field"
                                };

                                rsx! {
                                    div {
                                        key: "{record_id}",
                                        class: card_cls,

                                        if editing_record_id.read().as_ref() == Some(&record_id) {
                                            // 编辑模式
                                            div {
                                                class: "flex flex-col gap-2",
                                                input {
                                                    r#type: "text",
                                                    class: "field w-full bg-field border-accent text-ink text-xs",
                                                    value: "{edit_name_input.read()}",
                                                    oninput: move |event: Event<FormData>| {
                                                        edit_name_input.set(event.value());
                                                    },
                                                    onkeydown: {
                                                        let record_id = record_id.clone();
                                                        move |event: KeyboardEvent| {
                                                            if event.key() == Key::Enter {
                                                                handle_edit_confirm(record_id.clone());
                                                            } else if event.key() == Key::Escape {
                                                                handle_edit_cancel(());
                                                            }
                                                        }
                                                    },
                                                    autofocus: true,
                                                }
                                                div {
                                                    class: "flex gap-2",
                                                    button {
                                                        class: "btn-primary bg-accent text-white px-3 py-1.5 text-xs",
                                                        onclick: {
                                                            let record_id = record_id.clone();
                                                            move |_| handle_edit_confirm(record_id.clone())
                                                        },
                                                        {t!("btn.confirm").to_string()}
                                                    }
                                                    button {
                                                        class: "btn-secondary bg-transparent border-fieldline text-ink hover:border-muted2",
                                                        onclick: move |_| handle_edit_cancel(()),
                                                        {t!("btn.cancel").to_string()}
                                                    }
                                                }
                                            }
                                        } else {
                                            // 正常显示模式
                                            div {
                                                class: "cursor-pointer",
                                                onclick: move |_| handle_history_select(record_clone.clone()),
                                                div {
                                                    class: "flex items-center gap-2",
                                                    span {
                                                        class: "font-mono text-xs font-semibold text-ink truncate",
                                                        {record_name}
                                                    }
                                                    div { class: "flex-1" }
                                                    // 书签星标（始终可见）
                                                    button {
                                                        class: "leading-none text-sm shrink-0 rounded focus-visible:ring-2 focus-visible:ring-accent",
                                                        style: if record.bookmarked { "color:#e8a200" } else { "" },
                                                        title: if record.bookmarked { t!("history.unbookmark").to_string() } else { t!("history.bookmark").to_string() },
                                                        "aria-label": t!("history.toggle_bookmark_aria").to_string(),
                                                        onclick: {
                                                            let bm_id = record_id.clone();
                                                            move |event: Event<MouseData>| {
                                                                event.stop_propagation();
                                                                handle_toggle_bookmark(bm_id.clone());
                                                            }
                                                        },
                                                        if record.bookmarked { "★" } else { span { class: "text-muted2", "☆" } }
                                                    }
                                                    // 编辑 / 删除（hover 显示）
                                                    div {
                                                        class: "flex items-center gap-0.5 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 transition-opacity",
                                                        button {
                                                            class: "p-1 rounded text-muted2 hover:text-accent",
                                                            title: t!("history.rename_title").to_string(),
                                                            "aria-label": t!("history.rename_aria").to_string(),
                                                            onclick: {
                                                                let record = record.clone();
                                                                move |event: Event<MouseData>| {
                                                                    event.stop_propagation();
                                                                    handle_edit_click(record.clone());
                                                                }
                                                            },
                                                            {icon_pencil()}
                                                        }
                                                        button {
                                                            class: "p-1 rounded text-muted2 hover:text-danger",
                                                            title: t!("btn.delete_title").to_string(),
                                                            "aria-label": t!("history.delete_aria").to_string(),
                                                            onclick: {
                                                                let delete_id = record_id.clone();
                                                                move |event: Event<MouseData>| {
                                                                    event.stop_propagation();
                                                                    handle_history_delete(delete_id.clone());
                                                                }
                                                            },
                                                            {icon_trash()}
                                                        }
                                                    }
                                                }
                                                p {
                                                    class: "font-mono text-[10.5px] text-muted2 mt-1",
                                                    {record.formatted_created_at()}
                                                }
                                                p {
                                                    class: "font-mono text-[11px] text-muted mt-1.5 truncate",
                                                    {record.content_preview()}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                }
            }

            // ===== 右侧主区域 =====
            div {
                id: "ejv-main",
                class: "flex-1 min-w-0 flex flex-col overflow-hidden",

                // 顶部工具栏（窄屏横向滚动）
                header {
                    class: "ejv-scroll flex items-center gap-4 h-[58px] shrink-0 px-4 bg-headerbg border-b border-line overflow-x-auto overflow-y-hidden",

                    // 缩进
                    div {
                        class: "flex items-center gap-2 shrink-0",
                        span { class: "text-[11.5px] text-muted whitespace-nowrap", {t!("toolbar.indent_size").to_string()} }
                        select {
                            class: "field bg-field border-fieldline text-ink text-xs py-1.5 pr-2 cursor-pointer",
                            value: app_state.read().format_options.indent_size.to_string(),
                            onchange: move |event| {
                                if let Ok(size) = event.value().parse::<usize>() {
                                    app_state.write().format_options.indent_size = size;
                                    save_cfg();
                                    // 自动格式化开启时，设置变更立即重排输出（软格式化，不写历史）。
                                    if ui_settings.read().auto_format { auto_format_now(); }
                                }
                            },
                            option { value: "2", {t!("toolbar.spaces", n = 2).to_string()} }
                            option { value: "4", {t!("toolbar.spaces", n = 4).to_string()} }
                            option { value: "8", {t!("toolbar.spaces", n = 8).to_string()} }
                        }
                    }
                    // 排序键
                    label {
                        class: "flex items-center gap-1.5 text-[11.5px] text-muted cursor-pointer select-none whitespace-nowrap shrink-0",
                        title: t!("toolbar.sort_keys_title").to_string(),
                        input {
                            r#type: "checkbox",
                            class: "w-3.5 h-3.5 cursor-pointer",
                            checked: app_state.read().format_options.sort_keys,
                            onchange: move |event| {
                                app_state.write().format_options.sort_keys = event.checked();
                                save_cfg();
                                // 自动格式化开启时，切换排序键立即重排输出（软格式化，不写历史）。
                                if ui_settings.read().auto_format { auto_format_now(); }
                            },
                        }
                        span { {t!("toolbar.sort_keys").to_string()} }
                    }
                    // 字号
                    div {
                        class: "flex items-center gap-2 shrink-0",
                        span { class: "text-[11.5px] text-muted whitespace-nowrap", {t!("toolbar.font_size").to_string()} }
                        div {
                            class: "seg bg-field border-fieldline",
                            for (size, label) in [(13usize, t!("toolbar.font_small").to_string()), (14, t!("toolbar.font_medium").to_string()), (16, t!("toolbar.font_large").to_string())] {
                                button {
                                    key: "{size}",
                                    class: if ui_settings.read().font_size == size {
                                        "seg-item seg-on text-ink"
                                    } else {
                                        "seg-item text-muted hover:text-ink"
                                    },
                                    onclick: move |_| {
                                        ui_settings.write().font_size = size;
                                        save_cfg();
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                    // 主题
                    div {
                        class: "flex items-center gap-2 shrink-0",
                        span { class: "text-[11.5px] text-muted whitespace-nowrap", {t!("toolbar.theme").to_string()} }
                        div {
                            class: "seg bg-field border-fieldline",
                            button {
                                class: if ui_settings.read().theme != "dark" {
                                    "seg-item seg-on text-ink inline-flex items-center gap-1.5"
                                } else {
                                    "seg-item text-muted hover:text-ink inline-flex items-center gap-1.5"
                                },
                                title: t!("theme.light").to_string(),
                                onclick: move |_| {
                                    ui_settings.write().theme = "light".to_string();
                                    apply_theme("light");
                                    save_cfg();
                                },
                                {icon_sun()}
                                {t!("theme.light").to_string()}
                            }
                            button {
                                class: if ui_settings.read().theme == "dark" {
                                    "seg-item seg-on text-ink inline-flex items-center gap-1.5"
                                } else {
                                    "seg-item text-muted hover:text-ink inline-flex items-center gap-1.5"
                                },
                                title: t!("theme.dark").to_string(),
                                onclick: move |_| {
                                    ui_settings.write().theme = "dark".to_string();
                                    apply_theme("dark");
                                    save_cfg();
                                },
                                {icon_moon()}
                                {t!("theme.dark").to_string()}
                            }
                        }
                    }
                    // 密度（舒适 / 紧凑）：紧凑收紧格式化结果区行距
                    div {
                        class: "flex items-center gap-2 shrink-0",
                        span { class: "text-[11.5px] text-muted whitespace-nowrap", {t!("toolbar.density").to_string()} }
                        div {
                            class: "seg bg-field border-fieldline",
                            for (val, label) in [("comfortable", t!("toolbar.density_comfortable").to_string()), ("compact", t!("toolbar.density_compact").to_string())] {
                                button {
                                    key: "{val}",
                                    class: if ui_settings.read().density == val {
                                        "seg-item seg-on text-ink"
                                    } else {
                                        "seg-item text-muted hover:text-ink"
                                    },
                                    onclick: move |_| {
                                        ui_settings.write().density = val.to_string();
                                        save_cfg();
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                    // 自动格式化
                    label {
                        class: "flex items-center gap-1.5 text-[11.5px] text-muted cursor-pointer select-none whitespace-nowrap shrink-0",
                        title: t!("toolbar.auto_format_title").to_string(),
                        input {
                            r#type: "checkbox",
                            class: "w-3.5 h-3.5 cursor-pointer",
                            checked: ui_settings.read().auto_format,
                            onchange: move |event| {
                                ui_settings.write().auto_format = event.checked();
                                save_cfg();
                            },
                        }
                        span { {t!("toolbar.auto_format").to_string()} }
                    }

                    div { class: "flex-1" }

                    // 动作：清空 / 压缩 / 格式化
                    div {
                        class: "flex items-center gap-2 shrink-0",
                        button {
                            class: "btn-secondary bg-transparent border-fieldline text-ink hover:border-muted2",
                            title: t!("btn.clear_title").to_string(),
                            onclick: clear_all,
                            {icon_eraser()}
                            {t!("btn.clear").to_string()}
                        }
                        button {
                            class: "btn-secondary bg-transparent border-fieldline text-ink hover:border-muted2",
                            disabled: is_processing,
                            title: t!("btn.minify_title").to_string(),
                            onclick: move |_| process(true),
                            {icon_minify()}
                            {t!("btn.minify").to_string()}
                        }
                        button {
                            class: "btn-primary bg-accent text-white hover:brightness-110 shadow-sm",
                            disabled: is_processing,
                            onclick: move |_| process(false),
                            {icon_format()}
                            if is_processing {
                                {t!("btn.processing").to_string()}
                            } else {
                                {t!("btn.format").to_string()}
                            }
                        }
                    }
                }

                // 主内容区：左结果（2）/ 右输入（1）
                div {
                    class: "flex-1 min-h-0 flex overflow-hidden",

                    // ===== 结果区（默认 flex-[2]=2:1；输入面板被拖宽固定后改 flex-1 吃满剩余）=====
                    section {
                        class: if input_w.read().is_some() {
                            "flex-1 min-w-0 flex flex-col bg-panel overflow-hidden"
                        } else {
                            "flex-[2] min-w-0 flex flex-col bg-panel overflow-hidden"
                        },

                        // 标题行
                        div {
                            class: "flex items-center gap-3 h-[53px] px-4 border-b border-line2 shrink-0",
                            h2 { class: "text-[13px] font-bold whitespace-nowrap shrink-0", {t!("result.title").to_string()} }
                            if has_output && !has_error {
                                span { class: "pill text-[11px] font-semibold text-[#16a34a] shrink-0", style: "background:rgba(22,163,74,.1)", {t!("result.formatted_badge").to_string()} }
                            }
                            if has_error {
                                span { class: "pill text-[11px] font-semibold text-danger shrink-0", style: "background:rgba(225,87,78,.12)", {t!("result.invalid_badge").to_string()} }
                            }
                            div { class: "flex-1" }
                            if has_output {
                                // 视图切换：文本 / 树
                                div {
                                    class: "seg bg-field border-fieldline",
                                    button {
                                        class: if showing_text { "seg-item seg-on text-ink" } else { "seg-item text-muted hover:text-ink" },
                                        title: t!("view.text_title").to_string(),
                                        onclick: move |_| view_mode.set(ViewMode::Text),
                                        {t!("view.text").to_string()}
                                    }
                                    button {
                                        class: if !showing_text { "seg-item seg-on text-ink disabled:opacity-50" } else { "seg-item text-muted hover:text-ink disabled:opacity-50" },
                                        disabled: tree_over_cap,
                                        title: if tree_over_cap { t!("view.tree_disabled_title").to_string() } else { t!("view.tree_title").to_string() },
                                        onclick: move |_| view_mode.set(ViewMode::Tree),
                                        {t!("view.tree").to_string()}
                                    }
                                }
                                // 行号开关（独立开关，文本/树形视图均生效；持久化到 UiSettings，默认关）
                                div {
                                    class: "seg bg-field border-fieldline",
                                    button {
                                        class: if ui_settings.read().show_line_numbers { "seg-item seg-on text-ink" } else { "seg-item text-muted hover:text-ink" },
                                        title: t!("result.line_numbers_title").to_string(),
                                        "aria-label": t!("result.line_numbers").to_string(),
                                        onclick: move |_| {
                                            let v = ui_settings.read().show_line_numbers;
                                            ui_settings.write().show_line_numbers = !v;
                                            save_cfg();
                                        },
                                        {lucide_cls("w-4 h-4", &["M4 9h16", "M4 15h16", "M10 3 8 21", "M16 3 14 21"])}
                                    }
                                }
                                button {
                                    class: "btn-secondary bg-transparent border-fieldline text-ink hover:border-muted2",
                                    title: t!("result.copy_title").to_string(),
                                    onclick: move |_| do_copy(app_state.read().output_content.clone()),
                                    {icon_copy()}
                                    {t!("btn.copy").to_string()}
                                }
                                button {
                                    class: "btn-secondary bg-transparent border-fieldline text-ink hover:border-muted2",
                                    title: t!("result.download_title").to_string(),
                                    onclick: move |_| download_text("formatted.json", &app_state.read().output_content),
                                    {icon_download()}
                                    {t!("btn.download").to_string()}
                                }
                            }
                        }

                        // 控制行（搜索 + 折叠命令；仅有输出时）
                        if has_output {
                            div {
                                class: "ejv-scroll flex items-center gap-2 h-[53px] px-4 border-b border-line2 shrink-0 overflow-x-auto",
                                div {
                                    class: "relative flex-1 min-w-[90px]",
                                    span { class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted2 flex pointer-events-none", {icon_search()} }
                                    input {
                                        class: "field w-full bg-field border-fieldline text-ink text-xs pl-9 pr-3 py-2",
                                        r#type: "text",
                                        placeholder: t!("search.result_placeholder").to_string(),
                                        value: "{output_query_str}",
                                        oninput: move |event| {
                                            output_query.set(event.value());
                                            current_match.set(0);
                                        },
                                        onkeydown: move |event: KeyboardEvent| {
                                            if event.key() == Key::Enter {
                                                let delta = if event.modifiers().shift() { -1 } else { 1 };
                                                go_match(delta);
                                            }
                                        },
                                    }
                                }
                                div {
                                    class: "seg bg-field border-fieldline shrink-0",
                                    button {
                                        class: if *search_case_sensitive.read() { "seg-item seg-on text-ink font-semibold" } else { "seg-item text-muted hover:text-ink font-semibold" },
                                        title: t!("search.case_sensitive_title").to_string(),
                                        onclick: move |_| {
                                            let v = *search_case_sensitive.read();
                                            search_case_sensitive.set(!v);
                                            current_match.set(0);
                                        },
                                        "Aa"
                                    }
                                }
                                if search_active {
                                    span {
                                        class: "text-[11.5px] text-muted whitespace-nowrap min-w-[44px] text-center tabular-nums shrink-0",
                                        if active_match_count == 0 { {t!("search.no_match").to_string()} } else { "{cur_match_display} / {active_match_count}" }
                                    }
                                    button {
                                        class: "inline-flex items-center justify-center w-7 h-7 rounded-md border border-fieldline text-muted bg-transparent hover:text-ink hover:border-muted2 shrink-0 disabled:opacity-40",
                                        title: t!("search.prev_title").to_string(),
                                        disabled: active_match_count == 0,
                                        onclick: move |_| go_match(-1),
                                        {icon_chevron_up()}
                                    }
                                    button {
                                        class: "inline-flex items-center justify-center w-7 h-7 rounded-md border border-fieldline text-muted bg-transparent hover:text-ink hover:border-muted2 shrink-0 disabled:opacity-40",
                                        title: t!("search.next_title").to_string(),
                                        disabled: active_match_count == 0,
                                        onclick: move |_| go_match(1),
                                        {icon_chevron_down()}
                                    }
                                    button {
                                        class: "px-2 py-1 text-xs text-muted hover:text-ink rounded focus-visible:ring-2 focus-visible:ring-accent shrink-0",
                                        title: t!("search.clear_title").to_string(),
                                        "aria-label": t!("search.clear_aria").to_string(),
                                        onclick: move |_| {
                                            output_query.set(String::new());
                                            current_match.set(0);
                                        },
                                        "✕"
                                    }
                                }
                                // 折叠命令（仅树模式可树形渲染时）
                                if in_tree_mode {
                                    span { class: "w-px h-5 bg-line shrink-0" }
                                    if tree_rows.is_some() {
                                        button {
                                            class: "px-2.5 py-1.5 text-[11.5px] rounded-md border border-fieldline text-muted bg-transparent hover:text-ink hover:border-muted2 whitespace-nowrap shrink-0",
                                            title: t!("tree.expand_all_title").to_string(),
                                            onclick: move |_| collapsed_paths.set(HashSet::new()),
                                            {t!("tree.expand_all").to_string()}
                                        }
                                        button {
                                            class: "px-2.5 py-1.5 text-[11.5px] rounded-md border border-fieldline text-muted bg-transparent hover:text-ink hover:border-muted2 whitespace-nowrap shrink-0",
                                            title: t!("tree.collapse_all_title").to_string(),
                                            onclick: move |_| {
                                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&app_state.read().output_content) {
                                                    collapsed_paths.set(collect_container_paths(&v, 1));
                                                }
                                            },
                                            {t!("tree.collapse_all").to_string()}
                                        }
                                        button {
                                            class: "px-2.5 py-1.5 text-[11.5px] rounded-md border border-fieldline text-muted bg-transparent hover:text-ink hover:border-muted2 whitespace-nowrap shrink-0",
                                            title: t!("tree.collapse_l2_title").to_string(),
                                            onclick: move |_| {
                                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&app_state.read().output_content) {
                                                    collapsed_paths.set(collect_container_paths(&v, 2));
                                                }
                                            },
                                            {t!("tree.collapse_l2").to_string()}
                                        }
                                    } else {
                                        span {
                                            class: "px-2 py-1 text-[11px] text-num bg-field border border-line2 rounded-md whitespace-nowrap shrink-0",
                                            {t!("tree.fallback_notice").to_string()}
                                        }
                                    }
                                }
                            }
                        }

                        // 内容区
                        div {
                            class: "flex-1 min-h-0 relative",
                            if is_processing {
                                div { class: "absolute inset-0 flex items-center justify-center text-muted", {t!("result.processing").to_string()} }
                            } else if has_error {
                                // 错误态：告警图标 + 本地化错误信息
                                div {
                                    class: "absolute inset-0 flex flex-col items-center justify-center gap-3 p-8 text-center",
                                    div {
                                        class: "w-12 h-12 rounded-full grid place-items-center text-danger",
                                        style: "background:rgba(225,87,78,.13)",
                                        {lucide_cls("w-6 h-6", &["M12 8v5", "M12 16.5v.01", "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z"])}
                                    }
                                    if let Some(err) = app_state.read().error_message.clone() {
                                        div {
                                            class: "font-mono text-xs text-danger bg-field border border-line rounded-lg px-4 py-3 max-w-[520px] break-words whitespace-pre-wrap",
                                            "{err}"
                                        }
                                    }
                                }
                            } else if !has_output {
                                // 空态：花括号字形 + 单行提示
                                div {
                                    class: "absolute inset-0 flex flex-col items-center justify-center gap-3 p-8 text-center text-muted2 select-none",
                                    {lucide_cls("w-10 h-10", &["M8 3H7a2 2 0 0 0-2 2v4a2 2 0 0 1-2 2 2 2 0 0 1 2 2v4a2 2 0 0 0 2 2h1", "M16 3h1a2 2 0 0 1 2 2v4a2 2 0 0 0 2 2 2 2 0 0 0-2 2v4a2 2 0 0 1-2 2h-1"])}
                                    p { class: "text-sm", {t!("result.empty_hint").to_string()} }
                                }
                            } else if let Some(rows) = tree_rows.as_ref() {
                                // 树形视图：扁平渲染可见行（折叠子树不产出行 => DOM 数 == 可见行数）
                                div {
                                    class: "ejv-scroll absolute inset-0 overflow-auto font-mono px-2.5 py-3",
                                    style: "{code_font} line-height:{code_lh};",
                                    for (ri, row) in rows.iter().enumerate() {
                                        {
                                            let (key_segs, val_segs): (&[SearchSeg], &[SearchSeg]) = if search_active {
                                                let (ks, vs) = &tree_seg_rows[ri];
                                                (ks.as_deref().unwrap_or(&[]), vs.as_deref().unwrap_or(&[]))
                                            } else {
                                                (&[], &[])
                                            };
                                            // 当前匹配所在行 / 选中行的额外底色
                                            let row_is_cur = search_active
                                                && key_segs.iter().chain(val_segs.iter()).any(|(m, gi, _)| *m && *gi == cur_match);
                                            // 括号匹配高亮：选中某容器（路径 P）时，其开括号行(P)与闭括号行(P~c)同时高亮
                                            let is_selected = !row_is_cur && match selected_path.read().as_deref() {
                                                Some(sp) => if row.is_close { row.path.strip_suffix("~c").map_or(false, |c| c == sp) } else { row.path == sp },
                                                None => false,
                                            };
                                            let row_state = if row_is_cur { "ejv-cur" } else if is_selected { "ejv-sel" } else { "" };
                                            let close_bracket = if row.value_text == "{" { "}" } else { "]" };
                                            let depth = row.depth.min(40);
                                            // 行号 gutter 宽度：按可见行数位数定（≥2 位）。webkit2gtk 下 min-width/text-right 未真正定宽，
                                            // 故按最大位数左侧补空格成定长串（配 whitespace-pre + 等宽字体），使各行行号占同样宽度、内容对齐。
                                            let ln_digits = rows.len().to_string().len().max(2);
                                            let ln_label = format!("{:>w$}", ri + 1, w = ln_digits);
                                            let p_for_caret = row.path.clone();
                                            let p_for_sel = row.path.clone();
                                            // 闭括号行点击 → 选中其容器（剥去 ~c 后缀），使开/闭成对高亮
                                            let p_for_sel_close = row.path.strip_suffix("~c").unwrap_or(&row.path).to_string();
                                            let p_for_val = row.path.clone();
                                            let p_for_path = row.path.clone();
                                            rsx! {
                                                if row.is_close {
                                                    // 闭括号行：引导线 + 占位 + 闭括号(+尾逗号)；点击选中其容器（成对高亮），无三角/键/复制
                                                    div {
                                                        key: "{row.path}",
                                                        class: "ejv-row flex items-start pl-2 pr-2 {row_state}",
                                                        onclick: move |_| selected_path.set(Some(p_for_sel_close.clone())),
                                                        if ui_settings.read().show_line_numbers {
                                                            span { class: "shrink-0 self-stretch text-right text-muted2 select-none border-r border-line2 pr-2 mr-1 font-mono tabular-nums whitespace-pre", style: "min-width:{ln_digits}ch", "{ln_label}" }
                                                        }
                                                        for _gi in 0..depth {
                                                            span { key: "{_gi}", class: "shrink-0 self-stretch border-l border-guide", style: "width:{tree_indent_px}px;transform:translateX(8px)" }
                                                        }
                                                        span { class: "w-4 shrink-0" }
                                                        span { class: "text-punct", "{row.value_text}" }
                                                        if row.trailing_comma {
                                                            span { class: "text-punct", "," }
                                                        }
                                                    }
                                                } else {
                                                div {
                                                    key: "{row.path}",
                                                    class: "ejv-row group flex items-start pl-2 pr-2 {row_state}",
                                                    onclick: move |_| selected_path.set(Some(p_for_sel.clone())),
                                                    // 行号 gutter（开启时）：右对齐、与文本视图同一开关
                                                    if ui_settings.read().show_line_numbers {
                                                        span { class: "shrink-0 self-stretch text-right text-muted2 select-none border-r border-line2 pr-2 mr-1 font-mono tabular-nums whitespace-pre", style: "min-width:{ln_digits}ch", "{ln_label}" }
                                                    }
                                                    // 缩进引导线：每层一个定宽 border-l 占位，堆叠成连续竖线
                                                    for _gi in 0..depth {
                                                        span { key: "{_gi}", class: "shrink-0 self-stretch border-l border-guide", style: "width:{tree_indent_px}px;transform:translateX(8px)" }
                                                    }
                                                    // 三角（容器）/ 占位
                                                    if row.is_container {
                                                        span {
                                                            class: "w-4 shrink-0 cursor-pointer text-punct select-none text-center text-[10px]",
                                                            onclick: move |event: Event<MouseData>| {
                                                                event.stop_propagation();
                                                                collapsed_paths.with_mut(|s| {
                                                                    if !s.remove(&p_for_caret) { s.insert(p_for_caret.clone()); }
                                                                });
                                                            },
                                                            if row.collapsed { "▶" } else { "▼" }
                                                        }
                                                    } else {
                                                        span { class: "w-4 shrink-0" }
                                                    }
                                                    // 键 + 冒号
                                                    if let Some(k) = row.key_label.as_ref() {
                                                        span {
                                                            class: "text-key",
                                                            if search_active {
                                                                for (si, (is_m, gi, t)) in key_segs.iter().enumerate() {
                                                                    if *is_m {
                                                                        span { key: "k{si}", id: "ejv-match-{gi}", class: if *gi == cur_match { "ejv-mark-cur" } else { "ejv-mark" }, "{t}" }
                                                                    } else {
                                                                        span { key: "k{si}", "{t}" }
                                                                    }
                                                                }
                                                            } else {
                                                                "{k}"
                                                            }
                                                        }
                                                        span { class: "text-punct", ": " }
                                                    }
                                                    // 值：容器开括号(+折叠预览/计数) 或 标量
                                                    span {
                                                        class: "{row.value_class} break-words",
                                                        if search_active && !row.is_container {
                                                            for (si, (is_m, gi, t)) in val_segs.iter().enumerate() {
                                                                if *is_m {
                                                                    span { key: "v{si}", id: "ejv-match-{gi}", class: if *gi == cur_match { "ejv-mark-cur" } else { "ejv-mark" }, "{t}" }
                                                                } else {
                                                                    span { key: "v{si}", "{t}" }
                                                                }
                                                            }
                                                        } else {
                                                            "{row.value_text}"
                                                        }
                                                    }
                                                    // 折叠摘要：预览（对象）/ 省略号（数组）+ 闭括号 + 计数胶囊
                                                    if row.is_container && row.collapsed {
                                                        if let Some(preview) = row.collapsed_preview.as_ref() {
                                                            span { class: "text-muted2 italic truncate max-w-[320px]", " {preview} " }
                                                            span { class: "text-punct", "{close_bracket}" }
                                                        } else {
                                                            span { class: "text-punct", " … {close_bracket}" }
                                                        }
                                                        span {
                                                            class: "ml-2 inline-flex items-center rounded-full bg-field border border-line2 px-1.5 text-muted shrink-0",
                                                            style: "font-size:.78em;",
                                                            "{row.item_count}"
                                                        }
                                                    }
                                                    // 尾逗号：本节点非父容器末项时补 ,（展开容器恒 false，逗号落在其闭括号行）
                                                    if row.trailing_comma {
                                                        span { class: "text-punct", "," }
                                                    }
                                                    // hover 浮出：复制值 / 复制路径
                                                    span {
                                                        class: "ejv-acts ml-auto pl-3 inline-flex items-center gap-0.5 shrink-0",
                                                        button {
                                                            class: "ejv-ic p-1 rounded text-muted2",
                                                            title: t!("tree.copy_value").to_string(),
                                                            onclick: move |event: Event<MouseData>| {
                                                                event.stop_propagation();
                                                                let opts = app_state.read().format_options.clone();
                                                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&app_state.read().output_content) {
                                                                    if let Some(txt) = node_copy_text(&v, &p_for_val, &opts) {
                                                                        copy_to_clipboard(&txt);
                                                                        show_toast(t!("btn.copied").to_string());
                                                                    }
                                                                }
                                                            },
                                                            {icon_copy()}
                                                        }
                                                        button {
                                                            class: "ejv-ic p-1 rounded text-muted2",
                                                            title: t!("tree.copy_path").to_string(),
                                                            onclick: move |event: Event<MouseData>| {
                                                                event.stop_propagation();
                                                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&app_state.read().output_content) {
                                                                    if let Some(expr) = path_to_expr(&v, &p_for_path) {
                                                                        copy_to_clipboard(&expr);
                                                                        show_toast(expr);
                                                                    }
                                                                }
                                                            },
                                                            {icon_link()}
                                                        }
                                                    }
                                                }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if let Some(rows) = text_rows.as_ref() {
                                // 文本逐行视图：复用行数据（不折叠、带引号键、语法高亮），支持 hover 复制值/路径 + 括号匹配
                                div {
                                    class: "ejv-scroll absolute inset-0 overflow-auto font-mono px-4 py-3",
                                    style: "{code_font} line-height:{code_lh};",
                                    for (ri, row) in rows.iter().enumerate() {
                                        {
                                            let (key_segs, val_segs): (&[SearchSeg], &[SearchSeg]) = if search_active {
                                                let (ks, vs) = &text_seg_rows[ri];
                                                (ks.as_deref().unwrap_or(&[]), vs.as_deref().unwrap_or(&[]))
                                            } else {
                                                (&[], &[])
                                            };
                                            let row_is_cur = search_active
                                                && key_segs.iter().chain(val_segs.iter()).any(|(m, gi, _)| *m && *gi == cur_match);
                                            // 括号匹配：选中容器(P) → 其开括号行(P)与闭括号行(P~c)同时高亮
                                            let is_selected = !row_is_cur && match selected_path.read().as_deref() {
                                                Some(sp) => if row.is_close { row.path.strip_suffix("~c").map_or(false, |c| c == sp) } else { row.path == sp },
                                                None => false,
                                            };
                                            let row_state = if row_is_cur { "ejv-cur" } else if is_selected { "ejv-sel" } else { "" };
                                            let depth = row.depth.min(40);
                                            let indent_ch = depth * indent_size;
                                            // 行号按最大位数左侧补空格成定长串（配 whitespace-pre + 等宽字体定宽对齐，修 webkit2gtk 错位）
                                            let ln_digits = rows.len().to_string().len().max(2);
                                            let ln_label = format!("{:>w$}", ri + 1, w = ln_digits);
                                            let sel_target = if row.is_close { row.path.strip_suffix("~c").unwrap_or(&row.path).to_string() } else { row.path.clone() };
                                            let p_for_val = row.path.clone();
                                            let p_for_path = row.path.clone();
                                            rsx! {
                                                div {
                                                    key: "{row.path}",
                                                    class: "ejv-row group flex items-start px-2 {row_state}",
                                                    onclick: move |_| selected_path.set(Some(sel_target.clone())),
                                                    if ui_settings.read().show_line_numbers {
                                                        span { class: "shrink-0 self-stretch text-right text-muted2 select-none border-r border-line2 pr-2 mr-2 font-mono tabular-nums whitespace-pre", style: "min-width:{ln_digits}ch", "{ln_label}" }
                                                    }
                                                    // 缩进占位：depth × 缩进大小 个字符宽，与格式化文本对齐
                                                    span { class: "shrink-0", style: "width:{indent_ch}ch" }
                                                    if row.is_close {
                                                        span { class: "text-punct", "{row.value_text}" }
                                                    } else {
                                                        if let Some(k) = row.key_label.as_ref() {
                                                            span {
                                                                class: "text-key",
                                                                "\""
                                                                if search_active {
                                                                    for (si, (is_m, gi, t)) in key_segs.iter().enumerate() {
                                                                        if *is_m {
                                                                            span { key: "k{si}", id: "ejv-match-{gi}", class: if *gi == cur_match { "ejv-mark-cur" } else { "ejv-mark" }, "{t}" }
                                                                        } else {
                                                                            span { key: "k{si}", "{t}" }
                                                                        }
                                                                    }
                                                                } else {
                                                                    "{k}"
                                                                }
                                                                "\""
                                                            }
                                                            span { class: "text-punct", ": " }
                                                        }
                                                        if row.is_container {
                                                            span { class: "text-punct", "{row.value_text}" }
                                                        } else {
                                                            span {
                                                                class: "{row.value_class} break-words",
                                                                if search_active {
                                                                    for (si, (is_m, gi, t)) in val_segs.iter().enumerate() {
                                                                        if *is_m {
                                                                            span { key: "v{si}", id: "ejv-match-{gi}", class: if *gi == cur_match { "ejv-mark-cur" } else { "ejv-mark" }, "{t}" }
                                                                        } else {
                                                                            span { key: "v{si}", "{t}" }
                                                                        }
                                                                    }
                                                                } else {
                                                                    "{row.value_text}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if row.trailing_comma {
                                                        span { class: "text-punct", "," }
                                                    }
                                                    if !row.is_close {
                                                        span {
                                                            class: "ejv-acts ml-auto pl-3 inline-flex items-center gap-0.5 shrink-0",
                                                            button {
                                                                class: "ejv-ic p-1 rounded text-muted2",
                                                                title: t!("tree.copy_value").to_string(),
                                                                onclick: move |event: Event<MouseData>| {
                                                                    event.stop_propagation();
                                                                    let opts = app_state.read().format_options.clone();
                                                                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&app_state.read().output_content) {
                                                                        if let Some(txt) = node_copy_text(&v, &p_for_val, &opts) {
                                                                            copy_to_clipboard(&txt);
                                                                            show_toast(t!("btn.copied").to_string());
                                                                        }
                                                                    }
                                                                },
                                                                {icon_copy()}
                                                            }
                                                            button {
                                                                class: "ejv-ic p-1 rounded text-muted2",
                                                                title: t!("tree.copy_path").to_string(),
                                                                onclick: move |event: Event<MouseData>| {
                                                                    event.stop_propagation();
                                                                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&app_state.read().output_content) {
                                                                        if let Some(expr) = path_to_expr(&v, &p_for_path) {
                                                                            copy_to_clipboard(&expr);
                                                                            show_toast(expr);
                                                                        }
                                                                    }
                                                                },
                                                                {icon_link()}
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                // 文本视图（超大回退）：纯 <pre> + 可选行号 gutter（语法高亮 / 搜索高亮）
                                div {
                                    class: "ejv-scroll absolute inset-0 overflow-auto",
                                    div {
                                        // flex 行：左侧 sticky 行号 gutter（横向滚动时固定）+ 右侧内容；
                                        // min-w-min 让内容宽度驱动横向滚动条。
                                        class: "flex min-w-min",
                                        if ui_settings.read().show_line_numbers {
                                            pre {
                                                class: "shrink-0 sticky left-0 z-10 text-right select-none text-muted2 bg-panel border-r border-line2 whitespace-pre font-mono tabular-nums pl-3 pr-2 py-4",
                                                style: "{code_font} line-height:{code_lh}; margin:0;",
                                                "{line_numbers_str}"
                                            }
                                        }
                                        pre {
                                            class: "font-mono text-ink whitespace-pre px-4 py-4",
                                            style: "{code_font} line-height:{code_lh}; margin:0;",
                                            if search_active {
                                                for (i, (is_match, mi, text)) in search_segments.iter().enumerate() {
                                                    if *is_match {
                                                        span { key: "{i}", id: "ejv-match-{mi}", class: if *mi == cur_match { "ejv-mark-cur" } else { "ejv-mark" }, "{text}" }
                                                    } else {
                                                        span { key: "{i}", "{text}" }
                                                    }
                                                }
                                            } else if let Some(tokens) = highlight_tokens.as_ref() {
                                                for (idx, (cls, text)) in tokens.iter().enumerate() {
                                                    span { key: "{idx}", class: "{cls}", "{text}" }
                                                }
                                            } else {
                                                "{formatted_output}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 底部统计胶囊栏（仅有效输出时）
                        if has_output {
                            if let Some(total) = total_label {
                                div {
                                    class: "ejv-scroll flex items-center gap-1.5 px-4 h-10 border-t border-line2 shrink-0 overflow-x-auto",
                                    span { class: "pill bg-accentsoft text-accent font-bold", "{total}" }
                                    for (i, (label, dot)) in stat_pills.iter().enumerate() {
                                        span {
                                            key: "{i}",
                                            class: "pill bg-field border border-line2 text-muted",
                                            span { class: "w-[7px] h-[7px] rounded-full {dot}" }
                                            "{label}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ===== 可拖拽分割条：左右自由拖动，任一面板到 SPLIT_MIN(48px) 兜底（兼当左分隔线）=====
                    div {
                        class: "w-1.5 shrink-0 cursor-col-resize bg-line hover:bg-accent transition-colors",
                        onmousedown: move |_| {
                            split_dragging.set(true);
                            // 拖动起点测一次主区（#ejv-main）左右缘视口坐标（拖动期间不变）：左缘=侧栏右缘，右缘=输入面板右缘。
                            // 安全：脚本为固定字面量、无任何用户输入插值，仅读取 DOM 几何（两端通用，同 apply_theme）。
                            spawn(async move {
                                if let Ok(v) = document::eval(
                                    "const r = document.getElementById('ejv-main').getBoundingClientRect(); return [r.left, r.right];"
                                ).await {
                                    if let Some(l) = v.get(0).and_then(|n| n.as_f64()) { input_left.set(l); }
                                    if let Some(r) = v.get(1).and_then(|n| n.as_f64()) { input_right.set(r); }
                                }
                            });
                        },
                    }

                    // ===== 输入区（默认 flex-1；被拖宽后固定 width，下限 SPLIT_MIN=48px）=====
                    section {
                        id: "ejv-input-panel",
                        class: if input_w.read().is_some() {
                            "shrink-0 min-w-0 flex flex-col bg-panel overflow-hidden"
                        } else {
                            "flex-1 min-w-0 flex flex-col bg-panel overflow-hidden"
                        },
                        style: if let Some(w) = *input_w.read() { format!("width:{w}px") } else { String::new() },

                        // 标题行
                        div {
                            class: "flex items-center gap-2 h-[53px] px-4 border-b border-line2 shrink-0",
                            h2 { class: "text-[13px] font-bold whitespace-nowrap", {t!("input.title").to_string()} }
                            div { class: "flex-1" }
                            if has_output && !has_error {
                                span { class: "pill text-[11px] font-semibold text-[#16a34a]", style: "background:rgba(22,163,74,.1)", {t!("result.formatted_badge").to_string()} }
                            }
                            if has_error {
                                span { class: "pill text-[11px] font-semibold text-danger", style: "background:rgba(225,87,78,.12)", {t!("result.invalid_badge").to_string()} }
                            }
                        }

                        // 控制行：示例 / 导入 / 复制
                        div {
                            class: "ejv-scroll flex items-center gap-2 h-[53px] px-4 border-b border-line2 shrink-0 overflow-x-auto",
                            div { class: "flex-1" }
                            button {
                                class: "btn-secondary bg-transparent border-fieldline text-muted hover:text-ink hover:border-muted2",
                                title: t!("input.sample_title").to_string(),
                                onclick: load_sample,
                                {icon_sparkles()}
                                {t!("btn.sample").to_string()}
                            }
                            // 文件导入：Web <input type=file> / 桌面 rfd（唯一按平台分叉的 RSX 节点）
                            {
                                #[cfg(target_arch = "wasm32")]
                                let import_node = rsx! {
                                    label {
                                        class: "btn-secondary bg-transparent border-fieldline text-muted hover:text-ink hover:border-muted2 cursor-pointer focus-within:ring-2 focus-within:ring-accent",
                                        title: t!("input.import_title").to_string(),
                                        {icon_file_up()}
                                        {t!("btn.import").to_string()}
                                        input {
                                            r#type: "file",
                                            class: "hidden",
                                            accept: ".json,application/json,.txt,text/plain",
                                            onchange: move |evt| {
                                                if let Some(file) = evt.files().into_iter().next() {
                                                    spawn(async move {
                                                        if let Ok(contents) = file.read_string().await {
                                                            app_state.write().input_content = contents;
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                    }
                                };
                                #[cfg(not(target_arch = "wasm32"))]
                                let import_node = rsx! {
                                    button {
                                        class: "btn-secondary bg-transparent border-fieldline text-muted hover:text-ink hover:border-muted2",
                                        title: t!("input.import_title").to_string(),
                                        onclick: move |_| {
                                            spawn(async move {
                                                if let Some(handle) = rfd::AsyncFileDialog::new()
                                                    .add_filter("JSON", &["json", "txt"])
                                                    .pick_file()
                                                    .await
                                                {
                                                    if let Ok(contents) = String::from_utf8(handle.read().await) {
                                                        app_state.write().input_content = contents;
                                                    }
                                                }
                                            });
                                        },
                                        {icon_file_up()}
                                        {t!("btn.import").to_string()}
                                    }
                                };
                                import_node
                            }
                            if !app_state.read().input_content.is_empty() {
                                button {
                                    class: "btn-secondary bg-transparent border-fieldline text-muted hover:text-ink hover:border-muted2",
                                    title: t!("input.copy_title").to_string(),
                                    onclick: move |_| do_copy(app_state.read().input_content.clone()),
                                    {icon_copy()}
                                    {t!("btn.copy").to_string()}
                                }
                            }
                        }

                        // 文本域
                        div {
                            class: "flex-1 min-h-0 relative",
                            textarea {
                                class: "ejv-scroll absolute inset-0 w-full h-full resize-none border-0 outline-none bg-panel2 text-ink font-mono p-4",
                                style: "{code_font} line-height:1.7;",
                                placeholder: t!("input.placeholder").to_string(),
                                value: app_state.read().input_content.clone(),
                                spellcheck: false,
                                oninput: move |event| {
                                    {
                                        let mut s = app_state.write();
                                        s.input_content = event.value();
                                        s.current_record_id = None; // 手动改输入 → 高亮消失
                                    }
                                    if ui_settings.read().auto_format {
                                        *autofmt_seq.write() += 1;
                                        let seq = *autofmt_seq.read();
                                        spawn(async move {
                                            sleep_ms(600).await;
                                            if *autofmt_seq.read() == seq && ui_settings.read().auto_format {
                                                auto_format_now();
                                            }
                                        });
                                    }
                                },
                                // 格式化快捷键已上移到根 div 的 onkeydown（全局生效）；此处不再监听，避免冒泡重复触发。
                            }
                        }

                        // 底部状态栏：字符/行数 + 拖拽提示
                        div {
                            class: "flex items-center gap-4 px-4 h-10 border-t border-line2 text-xs text-muted2 font-mono shrink-0",
                            span { {input_stats} }
                            div { class: "flex-1" }
                            span {
                                class: "inline-flex items-center gap-1.5 whitespace-nowrap",
                                style: "font-family:system-ui,-apple-system,'Segoe UI','PingFang SC',sans-serif;",
                                {icon_file_up()}
                                {t!("input.drag_tip").to_string()}
                            }
                        }
                    }
                }
            }

            // ===== 全窗口拖拽遮罩 =====
            if *drag_over.read() {
                div {
                    class: "fixed inset-0 z-[300] flex items-center justify-center bg-black/50 pointer-events-none",
                    style: "backdrop-filter:blur(2px);",
                    div {
                        class: "flex flex-col items-center gap-3 px-12 py-9 border-2 border-dashed border-accent rounded-2xl bg-panel text-accent text-[15px] font-bold shadow-panel",
                        {lucide_cls("w-8 h-8", &["M12 16V4", "m7 9 5-5 5 5", "M5 20h14"])}
                        {t!("input.drop_hint").to_string()}
                    }
                }
            }

            // ===== Toast =====
            if *toast_open.read() {
                div {
                    class: "fixed left-1/2 bottom-7 -translate-x-1/2 z-[200] bg-ink text-panel text-[12.5px] font-semibold px-5 py-2.5 rounded-full shadow-panel max-w-[80vw] truncate",
                    "{toast_msg}"
                }
            }

            // ===== 功能介绍（About）弹窗 =====
            if *show_about.read() {
                div {
                    class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4",
                    onclick: move |_| show_about.set(false),
                    div {
                        class: "relative w-full max-w-lg max-h-[85vh] overflow-y-auto rounded-2xl bg-panel border border-line shadow-panel p-6",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        button {
                            class: "absolute top-3 right-3 p-1 text-muted hover:text-ink rounded focus-visible:ring-2 focus-visible:ring-accent",
                            title: t!("btn.close_title").to_string(),
                            "aria-label": t!("btn.close_title").to_string(),
                            onclick: move |_| show_about.set(false),
                            "✕"
                        }
                        h2 {
                            class: "text-xl font-bold text-accent mb-1 pr-6",
                            {t!("about.title").to_string()}
                        }
                        p {
                            class: "text-sm text-muted mb-4",
                            {t!("about.subtitle").to_string()}
                        }
                        div {
                            class: "flex flex-col gap-3",
                            for (icon, title, desc) in [
                                ("🔒", t!("about.privacy_title").to_string(), t!("about.privacy_desc").to_string()),
                                ("💾", t!("about.local_title").to_string(), t!("about.local_desc").to_string()),
                                ("⭐", t!("about.bookmark_title").to_string(), t!("about.bookmark_desc").to_string()),
                                ("🖥️", t!("about.desktop_title").to_string(), t!("about.desktop_desc").to_string()),
                                ("⚡", t!("about.light_title").to_string(), t!("about.light_desc").to_string()),
                            ] {
                                div {
                                    key: "{icon}",
                                    class: "flex items-start gap-3",
                                    div { class: "text-2xl shrink-0 leading-none", "{icon}" }
                                    div {
                                        class: "min-w-0",
                                        h3 { class: "text-sm font-semibold text-ink", {title} }
                                        p { class: "text-sm text-muted", {desc} }
                                    }
                                }
                            }
                        }
                        // 技术栈署名 + 源码仓库链接：与功能列表用上边框分隔，居中弱化呈现。
                        div {
                            class: "mt-5 pt-4 border-t border-line text-center",
                            p {
                                class: "text-xs text-muted",
                                {t!("about.tech_stack").to_string()}
                            }
                            // 当前运行的 release 版本（取自 Cargo.toml 编译期常量 VERSION）。
                            p {
                                class: "text-xs font-medium text-ink mt-1",
                                {t!("about.version", v = VERSION).to_string()}
                            }
                            // GitHub 源码链接。用 button 而非 <a href>：桌面 webview 内导航 href 会替换掉
                            // 应用页面，故走 open_url（桌面交系统浏览器 / Web 新标签）。title 悬停显示真实地址。
                            button {
                                class: "mt-3 inline-flex items-center gap-1 text-xs font-medium text-accent hover:underline rounded focus-visible:ring-2 focus-visible:ring-accent",
                                title: GITHUB_URL,
                                onclick: move |_| open_url(GITHUB_URL),
                                span { {t!("about.source").to_string()} }
                                span { "aria-hidden": "true", "↗" }
                            }
                        }
                    }
                }
            }

            // ===== 启动门控遮罩（FOUC 屏障）=====
            // SPLASH_CSS 无条件渲染（保证 keyframes/失败态样式始终在位、不依赖 Tailwind）；
            // 遮罩在 Loading/Failed 时覆盖在真实 UI 之上，Ready 时撤除。
            // 真实 UI（含 document::Stylesheet）始终挂在其下并被样式化，故探针能看到 CSS 生效——
            // 【勿】改为 if ready {app} else {splash} 的二选一渲染（那会让样式表脱离 DOM、永不加载而死锁）。
            style { dangerous_inner_html: SPLASH_CSS }
            match *load_phase.read() {
                LoadPhase::Ready => rsx! {},
                LoadPhase::Loading => rsx! {
                    div { id: "ejv-splash",
                        div { class: "ejv-ring" }
                    }
                },
                LoadPhase::Failed => rsx! {
                    div { id: "ejv-splash",
                        div { class: "ejv-fail",
                            div { class: "ejv-fail-title", {t!("splash.fail_title").to_string()} }
                            div { class: "ejv-fail-msg", {t!("splash.fail_msg").to_string()} }
                            button {
                                // 整页 reload：失败的样式表 <link> 不会自动重取，须重新加载页面重新拉取 CSS。
                                class: "ejv-retry",
                                onclick: move |_| { let _ = document::eval("window.location.reload();"); },
                                {t!("splash.retry").to_string()}
                            }
                        }
                    }
                },
            }
        }
    }
}
