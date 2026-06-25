// 增强版服务层模块
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::platform::Storage;
use sha1::{Sha1, Digest};
use std::collections::HashSet;
// HistoryRecord 定义移到这里，避免循环依赖

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HistoryRecord {
    pub id: String,
    pub name: String,
    pub content: String,
    pub formatted_content: String,
    pub created_at: String,
    /// 是否书签/收藏。`#[serde(default)]` 保证旧 localStorage 记录（无此字段）仍能反序列化为 false。
    #[serde(default)]
    pub bookmarked: bool,
}

impl HistoryRecord {
    /// 创建新的历史记录，使用基于 JSON 内容的 Git 风格短 hash 作为默认名称
    pub fn new(content: String, formatted_content: String) -> Self {
        let now = chrono::Utc::now();

        // 生成基于 JSON 内容的 Git 风格短 hash
        let hash = Self::generate_short_hash(&content);

        Self {
            id: Self::generate_id(now.timestamp_millis()),
            name: hash, // 使用基于内容的短 hash
            content,
            formatted_content,
            created_at: now.to_rfc3339(),
            bookmarked: false,
        }
    }

    /// 生成唯一 id：毫秒时间戳 + 进程内自增计数器，避免同一毫秒内的 id 碰撞
    /// （仍为 String，旧 localStorage 记录可正常反序列化，无需迁移）
    fn generate_id(millis: i64) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{}-{}", millis, n)
    }

    /// 生成基于内容的 Git 风格短 hash（7个字符）
    fn generate_short_hash(content: &str) -> String {
        let mut hasher = Sha1::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();

        // 转换为十六进制字符串并取前7个字符
        format!("{:x}", result)[..7].to_string()
    }

    /// 获取格式化的创建时间 (Y-m-d H:i:s)
    pub fn formatted_created_at(&self) -> String {
        if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(&self.created_at) {
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            self.created_at.clone()
        }
    }

    /// 获取内容的 trim 后的预览（前50个字符）
    pub fn content_preview(&self) -> String {
        let trimmed = self.content.trim();
        let chars: Vec<char> = trimmed.chars().collect();
        if chars.len() > 50 {
            format!("{}...", chars.iter().take(50).collect::<String>())
        } else {
            trimmed.to_string()
        }
    }
}

/// JSON 验证结果。
///
/// 错误以**结构化**形式返回（line/column/kind），**不在此拼接任何用户可见文案**——
/// service 层语言无关（被 lib crate、benches、`--no-default-features` 逻辑测试复用）。
/// 文案本地化收敛到 bin crate（`app.rs`），用 `t!()` 按 `kind` 组装。
#[derive(Clone, Debug, PartialEq)]
pub enum ValidationResult {
    Valid,
    Invalid {
        /// serde_json 报告的行号（1 起）；`Empty` 时为 0（无意义占位）
        line: usize,
        /// serde_json 报告的列号（1 起）；`Empty` 时为 0
        column: usize,
        kind: ValidationErrorKind,
    },
}

/// 校验错误的类别。翻译在 `app.rs` 按此分派；`Syntax` 携带 serde_json 的原始英文描述
/// （仅作兜底展示，语言无关——不构成 service 层的语言耦合）。
#[derive(Clone, Debug, PartialEq)]
pub enum ValidationErrorKind {
    /// 输入为空（或仅含空白）
    Empty,
    /// JSON 不完整（EOF，可能缺少结束符号）
    Incomplete,
    /// 其它语法错误，携带 serde_json 的原始描述（如 "expected value at line 1 column 5"）
    Syntax(String),
}

/// 格式化选项
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormatOptions {
    pub indent_size: usize,
    /// 是否按字母序排序对象键（默认 false，保留原始键序）
    #[serde(default)]
    pub sort_keys: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_size: 4,
            sort_keys: false,
        }
    }
}

/// JSON 处理服务
pub struct JsonService;

impl JsonService {
    /// 验证 JSON 字符串
    pub fn validate(json_str: &str) -> ValidationResult {
        if json_str.trim().is_empty() {
            return ValidationResult::Invalid {
                line: 0,
                column: 0,
                kind: ValidationErrorKind::Empty,
            };
        }

        match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(_) => ValidationResult::Valid,
            Err(e) => {
                let kind = if e.is_eof() {
                    ValidationErrorKind::Incomplete
                } else {
                    // 保留 serde_json 的原始英文描述（含其自带的 "at line.. column.."），
                    // 由 app.rs 在展示时剥离冗余后缀并本地化包裹。
                    ValidationErrorKind::Syntax(e.to_string())
                };
                ValidationResult::Invalid { line: e.line(), column: e.column(), kind }
            }
        }
    }

    /// 格式化 JSON 字符串
    pub fn format(json_str: &str, options: &FormatOptions) -> Result<String> {
        let mut value: serde_json::Value = serde_json::from_str(json_str)?;
        if options.sort_keys {
            value = Self::sort_value(value);
        }

        let indent = vec![b' '; options.indent_size];
        let formatter = serde_json::ser::PrettyFormatter::with_indent(&indent);

        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        value.serialize(&mut ser)?;

        Ok(String::from_utf8(buf)?)
    }

    /// 压缩 JSON（移除空白）
    pub fn minify(json_str: &str) -> Result<String> {
        let value: serde_json::Value = serde_json::from_str(json_str)?;
        Ok(serde_json::to_string(&value)?)
    }

    /// 递归按字母序排序对象键（数组顺序保持不变）。
    /// 依赖 serde_json 的 preserve_order 特性：重建的 Map 会保持此处的插入顺序。
    fn sort_value(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut entries: Vec<(String, serde_json::Value)> = map
                    .into_iter()
                    .map(|(k, v)| (k, Self::sort_value(v)))
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                serde_json::Value::Object(entries.into_iter().collect())
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.into_iter().map(Self::sort_value).collect())
            }
            other => other,
        }
    }

    /// 获取 JSON 统计信息
    pub fn get_stats(json_str: &str) -> Result<JsonStats> {
        let value: serde_json::Value = serde_json::from_str(json_str)?;
        let mut stats = JsonStats::default();
        Self::count_value(&value, &mut stats);
        Ok(stats)
    }

    fn count_value(value: &serde_json::Value, stats: &mut JsonStats) {
        match value {
            serde_json::Value::Object(obj) => {
                stats.objects += 1;
                stats.keys += obj.len();
                for v in obj.values() {
                    Self::count_value(v, stats);
                }
            }
            serde_json::Value::Array(arr) => {
                stats.arrays += 1;
                stats.array_items += arr.len();
                for v in arr {
                    Self::count_value(v, stats);
                }
            }
            serde_json::Value::String(_) => stats.strings += 1,
            serde_json::Value::Number(_) => stats.numbers += 1,
            serde_json::Value::Bool(_) => stats.booleans += 1,
            serde_json::Value::Null => stats.nulls += 1,
        }
    }
}

/// JSON 统计信息
#[derive(Clone, Debug, Default)]
pub struct JsonStats {
    pub objects: usize,
    pub arrays: usize,
    pub keys: usize,
    pub array_items: usize,
    pub strings: usize,
    pub numbers: usize,
    pub booleans: usize,
    pub nulls: usize,
}

impl JsonStats {
    pub fn total_values(&self) -> usize {
        self.objects + self.arrays + self.strings + self.numbers + self.booleans + self.nulls
    }
}

/// 历史记录服务
pub struct HistoryService;

impl HistoryService {
    const STORAGE_KEY: &'static str = "easy_json_view_history";
    pub const MAX_RECORDS: usize = 100; // 最大保存记录数

    /// 加载历史记录
    pub async fn load_history() -> Result<Vec<HistoryRecord>> {
        match Storage::get::<Vec<HistoryRecord>>(Self::STORAGE_KEY) {
            Ok(records) => Ok(records),
            Err(_) => Ok(Vec::new()),
        }
    }

    /// 保存历史记录
    pub async fn save_record(record: &HistoryRecord) -> Result<()> {
        let mut records = Self::load_history().await?;

        // 若相同内容的旧记录曾被加书签，则在去重后保留书签标志（避免重复格式化导致书签丢失）
        let was_bookmarked = records
            .iter()
            .any(|r| r.content == record.content && r.bookmarked);

        // 去重：移除内容完全相同的旧记录，避免重复格式化刷屏并挤掉有用记录
        records.retain(|r| r.content != record.content);

        // 添加新记录到开头（携带书签标志）
        let mut to_insert = record.clone();
        to_insert.bookmarked = record.bookmarked || was_bookmarked;
        records.insert(0, to_insert);

        // 限制记录数量：书签记录永不被淘汰，剩余名额按新→旧填充非书签记录
        if records.len() > Self::MAX_RECORDS {
            let bookmarked_count = records.iter().filter(|r| r.bookmarked).count();
            let mut budget = Self::MAX_RECORDS.saturating_sub(bookmarked_count);
            records.retain(|r| {
                if r.bookmarked {
                    true
                } else if budget > 0 {
                    budget -= 1;
                    true
                } else {
                    false
                }
            });
        }

        Storage::set(Self::STORAGE_KEY, &records)
            .map_err(|e| anyhow::anyhow!("保存历史记录失败: {:?}", e))?;

        Ok(())
    }

    /// 切换某条记录的书签状态，持久化后返回新的状态
    pub async fn toggle_bookmark(record_id: &str) -> Result<bool> {
        let mut records = Self::load_history().await?;
        let new_state = if let Some(r) = records.iter_mut().find(|r| r.id == record_id) {
            r.bookmarked = !r.bookmarked;
            r.bookmarked
        } else {
            return Err(anyhow::anyhow!("未找到指定的历史记录"));
        };
        Storage::set(Self::STORAGE_KEY, &records)
            .map_err(|e| anyhow::anyhow!("更新书签状态失败: {:?}", e))?;
        Ok(new_state)
    }

    /// 删除历史记录
    pub async fn delete_record(record_id: &str) -> Result<()> {
        let mut records = Self::load_history().await?;
        records.retain(|r| r.id != record_id);
        
        Storage::set(Self::STORAGE_KEY, &records)
            .map_err(|e| anyhow::anyhow!("删除历史记录失败: {:?}", e))?;
        
        Ok(())
    }

    /// 清空所有历史记录
    pub async fn clear_history() -> Result<()> {
        Storage::delete(Self::STORAGE_KEY);
        Ok(())
    }

    /// 更新历史记录名称
    pub async fn update_record_name(record_id: &str, new_name: String) -> Result<()> {
        let mut records = Self::load_history().await?;

        if let Some(record) = records.iter_mut().find(|r| r.id == record_id) {
            record.name = new_name;

            Storage::set(Self::STORAGE_KEY, &records)
                .map_err(|e| anyhow::anyhow!("更新历史记录名称失败: {:?}", e))?;
        } else {
            return Err(anyhow::anyhow!("未找到指定的历史记录"));
        }

        Ok(())
    }

    /// 就地更新某条记录的格式化结果（formatted_content），保留 id/名称/内容/时间戳/书签。
    /// 用于「trim 去重」命中已存记录、但本次缩进选项变化致输出不同时刷新其美化结果——
    /// 不新增、不置顶。纯逻辑收口在 `set_record_formatted`（便于单测、不触盘）。
    pub async fn update_record_formatted(record_id: &str, formatted: String) -> Result<()> {
        let mut records = Self::load_history().await?;

        if set_record_formatted(&mut records, record_id, formatted) {
            Storage::set(Self::STORAGE_KEY, &records)
                .map_err(|e| anyhow::anyhow!("更新历史记录格式化结果失败: {:?}", e))?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("未找到指定的历史记录"))
        }
    }

}

/// 在记录列表中就地更新 id 匹配项的 `formatted_content`，返回是否命中。
/// 纯函数（无 Storage / async）：仅动目标项的 formatted_content，其它字段与其它记录均不变——
/// 与 `HistoryService::update_record_formatted` 的存储 IO 解耦，便于单测。
pub fn set_record_formatted(records: &mut [HistoryRecord], record_id: &str, formatted: String) -> bool {
    if let Some(record) = records.iter_mut().find(|r| r.id == record_id) {
        record.formatted_content = formatted;
        true
    } else {
        false
    }
}

/// 配置服务
pub struct ConfigService;

impl ConfigService {
    const CONFIG_KEY: &'static str = "easy_json_view_config";

    /// 加载配置
    pub async fn load_config() -> Result<AppConfig> {
        match Storage::get::<AppConfig>(Self::CONFIG_KEY) {
            Ok(config) => Ok(config),
            Err(_) => Ok(AppConfig::default()),
        }
    }

    /// 保存配置
    pub async fn save_config(config: &AppConfig) -> Result<()> {
        Storage::set(Self::CONFIG_KEY, config)
            .map_err(|e| anyhow::anyhow!("保存配置失败: {:?}", e))?;
        Ok(())
    }
}

/// 应用配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    /// `#[serde(default)]`：前向兼容——即使某次只写入了部分字段，反序列化也能各自回退到默认值，
    /// 不会因缺字段而整体解析失败丢配置。
    #[serde(default)]
    pub format_options: FormatOptions,
    #[serde(default)]
    pub ui_settings: UiSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            format_options: FormatOptions::default(),
            ui_settings: UiSettings::default(),
        }
    }
}

/// 界面语言默认值（与 i18n! 的 fallback、locales 文件名严格一致："en"）。
/// 抽成函数以同时用于 `#[serde(default)]` 与 `Default`，避免两处字面量漂移。
fn default_language() -> String {
    "en".to_string()
}

/// 显示密度默认值（"comfortable" / "compact"）。抽成函数以同时用于 `#[serde(default)]` 与 `Default`。
/// 仅作为语言无关的字符串枚举存储；UI 据此调整格式化结果区的行距。
fn default_density() -> String {
    "comfortable".to_string()
}

/// UI 设置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiSettings {
    pub theme: String,
    pub font_size: usize,
    pub auto_format: bool,
    /// 界面语言（"en" / "zh-CN"）。`#[serde(default)]` 保证旧 store/localStorage（无此字段）
    /// 反序列化为默认英文，向后兼容。本字段语言无关（仅是 locale code），故仍属 service 层。
    #[serde(default = "default_language")]
    pub language: String,
    /// 显示密度（"comfortable" / "compact"）。`#[serde(default)]` 让旧配置回退默认舒适。
    /// 紧凑模式收紧格式化结果区（树/文本）的行距，便于一屏看更多内容。
    #[serde(default = "default_density")]
    pub density: String,
    /// 文本视图是否显示行号。`#[serde(default)]` → 旧配置回退 false（默认不显示）。
    /// 仅作用于「文本」结果视图；树形视图无行号概念。
    #[serde(default)]
    pub show_line_numbers: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "light".to_string(),
            font_size: 14,
            auto_format: true,
            language: default_language(),
            density: default_density(),
            show_line_numbers: false,
        }
    }
}

// ========== 树形视图（可折叠）==========
//
// 渲染策略：把当前折叠状态下的「可见行」前序遍历成一个扁平 `Vec<TreeRow>`，
// UI 直接平铺渲染。折叠的容器只产出一行摘要、**不递归子节点** => DOM 节点数
// 等于可见行数，天然防止深层/大体量 JSON 撑爆 DOM。与本仓库既有的
// 「预计算一次再渲染」惯例（highlight_json / search_segments）一致。

/// 树形视图的一行（已展平）。`path` 同时用作渲染 key 与折叠集合的键。
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    /// 缩进层级：root = 0，顶层键 = 1，依此类推
    pub depth: usize,
    /// 节点路径（按位置编号，见 `child_path`），唯一且与键名内容无关
    pub path: String,
    /// 对象成员显示其键名（不含引号）；数组元素与根为 None
    pub key_label: Option<String>,
    /// 是否为非空容器（可折叠、显示三角）
    pub is_container: bool,
    /// 该容器当前是否折叠（仅 is_container 为真时有意义）
    pub collapsed: bool,
    /// 容器的直接子项数（用于折叠摘要）
    pub item_count: usize,
    /// 值部分的 Tailwind 颜色类（复用语法高亮配色）
    pub value_class: &'static str,
    /// 值部分文本：标量值 / 容器开括号（"{" 或 "["）。
    /// 注意：容器（折叠或展开）此处统一只放**开括号**；折叠摘要/计数由 UI 用
    /// `collapsed` + `collapsed_preview` + `item_count` 组装（见 app.rs 树渲染）。
    pub value_text: String,
    /// 折叠对象的键预览（首 ≤4 键，多则补 "…"）；折叠数组与非折叠/非容器行为 None。
    pub collapsed_preview: Option<String>,
    /// 是否为「仅闭括号」行（展开容器递归完子节点后补的 `}` / `]` 行）。
    /// 闭行无键、非容器、不可折叠、不参与搜索；`value_text` 为闭括号字符。
    pub is_close: bool,
    /// 该行内容末尾是否补 `,`（即本节点在其父容器中不是最后一个子项）。
    /// 标量/折叠摘要：逗号紧跟值/摘要；展开容器：开括号行恒 false、逗号落在其闭括号行。
    pub trailing_comma: bool,
}

/// 子节点路径生成约定：父路径 + "." + 在父容器中的位置序号。
/// 按位置编号而非键名 => 键含 `.` 等特殊字符也不会与其它路径冲突。
/// `build_tree_rows` 与 `collect_container_paths` **必须**共用此函数，否则
/// 两者产生的路径不一致，折叠命令会静默失效。
fn child_path(parent: &str, index: usize) -> String {
    format!("{}.{}", parent, index)
}

/// 将 JSON 值展平为当前折叠状态下的可见行列表（前序遍历）。
///
/// - 非空 Object/Array = 容器：折叠时产出一行摘要且不递归；展开时产出开头行后递归子节点。
/// - 空 `{}` / `[]` 作为字面量行（无三角、不可折叠）。
/// - 标量（字符串保留引号）按语法高亮配色着色。
/// - `node_cap`：累计行数达到上限即停止，作为防御性兜底（正常调用方会先按节点数禁用树）。
pub fn build_tree_rows(
    value: &serde_json::Value,
    collapsed: &HashSet<String>,
    node_cap: usize,
) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    // root 是「最后一项」（无父容器）→ is_last=true，不产生尾逗号。
    walk_tree(value, 0, String::new(), None, true, collapsed, node_cap, &mut rows);
    rows
}

fn walk_tree(
    value: &serde_json::Value,
    depth: usize,
    path: String,
    key_label: Option<String>,
    is_last: bool,
    collapsed: &HashSet<String>,
    node_cap: usize,
    rows: &mut Vec<TreeRow>,
) {
    if rows.len() >= node_cap {
        return;
    }
    // 本节点是否需要尾逗号：在其父容器中不是最后一个子项。
    // 展开容器：逗号落在其【闭括号行】（开括号行恒不带）；折叠容器/标量：逗号紧跟本行。
    let comma = !is_last;
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            let count = map.len();
            let is_collapsed = collapsed.contains(&path);
            // 折叠对象内联预览首 ≤4 键，便于扫读（数组无键不预览）
            let preview = if is_collapsed { Some(object_key_preview(map)) } else { None };
            rows.push(TreeRow {
                depth, path: path.clone(), key_label,
                is_container: true, collapsed: is_collapsed, item_count: count,
                value_class: "text-punct",
                value_text: "{".to_string(),
                collapsed_preview: preview,
                is_close: false,
                trailing_comma: is_collapsed && comma,
            });
            if !is_collapsed {
                for (i, (k, v)) in map.iter().enumerate() {
                    if rows.len() >= node_cap { break; }
                    walk_tree(v, depth + 1, child_path(&path, i), Some(k.clone()), i == count - 1, collapsed, node_cap, rows);
                }
                // 展开容器：递归完子节点后补一行同深度闭括号（逗号随父级位置）。
                if rows.len() < node_cap {
                    rows.push(close_row(depth, &path, "}", comma));
                }
            }
        }
        serde_json::Value::Array(arr) if !arr.is_empty() => {
            let count = arr.len();
            let is_collapsed = collapsed.contains(&path);
            rows.push(TreeRow {
                depth, path: path.clone(), key_label,
                is_container: true, collapsed: is_collapsed, item_count: count,
                value_class: "text-punct",
                value_text: "[".to_string(),
                collapsed_preview: None,
                is_close: false,
                trailing_comma: is_collapsed && comma,
            });
            if !is_collapsed {
                for (i, v) in arr.iter().enumerate() {
                    if rows.len() >= node_cap { break; }
                    walk_tree(v, depth + 1, child_path(&path, i), None, i == count - 1, collapsed, node_cap, rows);
                }
                if rows.len() < node_cap {
                    rows.push(close_row(depth, &path, "]", comma));
                }
            }
        }
        // 标量与空容器：单行字面量（无三角、不可折叠）
        other => {
            let (value_class, value_text) = scalar_repr(other);
            rows.push(TreeRow {
                depth, path, key_label,
                is_container: false, collapsed: false, item_count: 0,
                value_class, value_text,
                collapsed_preview: None,
                is_close: false,
                trailing_comma: comma,
            });
        }
    }
}

/// 构造一行「仅闭括号」行：path 用 `{容器path}~c`（绝不与节点路径冲突，~ 不出现在 child_path 的数字段中）。
fn close_row(depth: usize, container_path: &str, bracket: &'static str, trailing_comma: bool) -> TreeRow {
    TreeRow {
        depth,
        path: format!("{}~c", container_path),
        key_label: None,
        is_container: false,
        collapsed: false,
        item_count: 0,
        value_class: "text-punct",
        value_text: bracket.to_string(),
        collapsed_preview: None,
        is_close: true,
        trailing_comma,
    }
}

/// 折叠对象的键预览：首 ≤4 键以 ", " 连接，多于 4 个补 ", …"。
fn object_key_preview(map: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut s = map.keys().take(4).cloned().collect::<Vec<_>>().join(", ");
    if map.len() > 4 {
        s.push_str(", …");
    }
    s
}

/// 标量 / 空容器的着色与文本表示（与 highlight_json 配色一致；字符串保留引号并正确转义）。
/// 颜色为 prototype 语义色名（text-str/num/bool/null/punct），深色由 input.css 的 html.dark 重映射。
fn scalar_repr(value: &serde_json::Value) -> (&'static str, String) {
    match value {
        serde_json::Value::String(s) => (
            "text-str",
            serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s)),
        ),
        serde_json::Value::Number(n) => ("text-num", n.to_string()),
        serde_json::Value::Bool(b) => ("text-bool", b.to_string()),
        serde_json::Value::Null => ("text-null", "null".to_string()),
        serde_json::Value::Object(_) => ("text-punct", "{}".to_string()),
        serde_json::Value::Array(_) => ("text-punct", "[]".to_string()),
    }
}

/// 收集所有「深度 ≥ min_depth」的非空容器路径，用于全局折叠命令：
/// - 折叠全部 => min_depth = 1（保留 root 展开，顶层键值折叠为摘要）
/// - 折叠全部二级字段 => min_depth = 2（root + 顶层键 + 其直接子级可见，更深折叠）
///
/// 深度约定与 `build_tree_rows` 完全一致（root = 0），并共用 `child_path`，
/// 保证产出的路径能精确命中折叠集合。
pub fn collect_container_paths(value: &serde_json::Value, min_depth: usize) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_paths(value, 0, String::new(), min_depth, &mut set);
    set
}

fn collect_paths(
    value: &serde_json::Value,
    depth: usize,
    path: String,
    min_depth: usize,
    set: &mut HashSet<String>,
) {
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            if depth >= min_depth {
                set.insert(path.clone());
            }
            for (i, (_, v)) in map.iter().enumerate() {
                collect_paths(v, depth + 1, child_path(&path, i), min_depth, set);
            }
        }
        serde_json::Value::Array(arr) if !arr.is_empty() => {
            if depth >= min_depth {
                set.insert(path.clone());
            }
            for (i, v) in arr.iter().enumerate() {
                collect_paths(v, depth + 1, child_path(&path, i), min_depth, set);
            }
        }
        _ => {}
    }
}

// ========== 结果区键值搜索（文本视图与树形视图共用） ==========

/// 渲染时最多标记的匹配数，防止在大文档中输入常见字符产生海量 <mark> 节点。
pub const MAX_SEARCH_MARKS: usize = 1000;

/// 在 haystack 中查找 needle 的全部非重叠匹配，返回字节区间 (start, end)。
/// 字节级比较，匹配边界与 needle 对齐 => 落在合法 UTF-8 字符边界上；
/// case_sensitive=false 时按 ASCII 大小写不敏感（非 ASCII 字节按原样比较），不改变字节长度。
///
/// 文本视图（切分高亮段）与树形视图（逐行键/值高亮、展开判断）**共用此函数**，
/// 确保「命中」口径在两端完全一致。
pub fn find_matches(haystack: &str, needle: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    if n.is_empty() || n.len() > h.len() {
        return out;
    }
    let eq = |a: u8, b: u8| if case_sensitive { a == b } else { a.eq_ignore_ascii_case(&b) };
    let mut i = 0;
    while i + n.len() <= h.len() {
        if (0..n.len()).all(|k| eq(h[i + k], n[k])) {
            out.push((i, i + n.len()));
            i += n.len(); // 非重叠
            if out.len() >= MAX_SEARCH_MARKS {
                break;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// 文本是否包含 needle（与 `find_matches` 同口径），用于树形展开判断。
fn text_contains(haystack: &str, needle: &str, case_sensitive: bool) -> bool {
    !find_matches(haystack, needle, case_sensitive).is_empty()
}

/// 树形视图「自动展开命中项」：返回为了让所有命中节点可见、必须保持展开的**容器 path** 集合。
///
/// 命中口径（与逐行高亮一致）：
/// - 任意带键的节点：其**键名**包含 query 即命中；
/// - **叶子**（标量/空容器）：其 `scalar_repr` 文本（字符串含引号）包含 query 即命中；
/// - 非空容器仅按键命中，不按其括号/摘要文本命中。
///
/// 命中节点的**全部祖先容器 path** 被收集（节点自身不收集——容器无需展开自身即可显示其行）。
/// 路径方案严格复用 `child_path`（root = ""），故产出的 path 能与 `collapsed_paths` 精确做差集。
/// 空 query 或无命中 => 空集合。
pub fn collect_search_expansions(
    value: &serde_json::Value,
    query: &str,
    case_sensitive: bool,
) -> HashSet<String> {
    let mut needed = HashSet::new();
    if query.is_empty() {
        return needed;
    }
    let mut stack: Vec<String> = Vec::new();
    walk_search(value, String::new(), None, query, case_sensitive, &mut stack, &mut needed);
    needed
}

fn walk_search(
    value: &serde_json::Value,
    path: String,
    key_label: Option<&str>,
    query: &str,
    cs: bool,
    stack: &mut Vec<String>,
    needed: &mut HashSet<String>,
) {
    // 本节点是否命中：键命中，或（仅叶子）标量文本命中。
    let key_hit = key_label.map_or(false, |k| text_contains(k, query, cs));
    let val_hit = match value {
        serde_json::Value::Object(map) if !map.is_empty() => false,
        serde_json::Value::Array(arr) if !arr.is_empty() => false,
        other => text_contains(&scalar_repr(other).1, query, cs),
    };
    if key_hit || val_hit {
        for anc in stack.iter() {
            needed.insert(anc.clone());
        }
    }
    // 递归非空容器：将自身 path 压栈作为后代的祖先。
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            stack.push(path.clone());
            for (i, (k, v)) in map.iter().enumerate() {
                walk_search(v, child_path(&path, i), Some(k.as_str()), query, cs, stack, needed);
            }
            stack.pop();
        }
        serde_json::Value::Array(arr) if !arr.is_empty() => {
            stack.push(path.clone());
            for (i, v) in arr.iter().enumerate() {
                walk_search(v, child_path(&path, i), None, query, cs, stack, needed);
            }
            stack.pop();
        }
        _ => {}
    }
}

// ========== 节点级操作：复制值 / 复制路径（树行 hover 动作） ==========
//
// 路径方案严格复用 `child_path`（root = ""，子节点 = "父.位置序号"）。
// 两函数按位置序号回溯 serde_json::Value（保留 preserve_order 的插入序），语言无关。

/// 键是否为 JSONPath 点号可直接拼接的标识符：`^[A-Za-z_$][A-Za-z0-9_$]*$`。
fn is_ident(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// 按位置路径从根值走到目标节点；任一步越界 / 类型不符 => None。
fn node_at_path<'a>(root: &'a serde_json::Value, positional_path: &str) -> Option<&'a serde_json::Value> {
    let mut cur = root;
    for seg in positional_path.split('.') {
        if seg.is_empty() {
            continue; // 跳过 root 的空首段（路径以 '.' 起）
        }
        let idx: usize = seg.parse().ok()?;
        cur = match cur {
            serde_json::Value::Object(map) => map.iter().nth(idx).map(|(_, v)| v)?,
            serde_json::Value::Array(arr) => arr.get(idx)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// 把位置路径回溯为 JSONPath 风格表达式（如 `$.data.items[0].id`）。
/// 对象成员按键名（标识符安全 → `.key`，否则 `["key"]`），数组成员按下标 `[i]`。
/// 路径非法 => None。
pub fn path_to_expr(root: &serde_json::Value, positional_path: &str) -> Option<String> {
    let mut cur = root;
    let mut out = String::from("$");
    for seg in positional_path.split('.') {
        if seg.is_empty() {
            continue;
        }
        let idx: usize = seg.parse().ok()?;
        match cur {
            serde_json::Value::Object(map) => {
                let (k, v) = map.iter().nth(idx)?;
                if is_ident(k) {
                    out.push('.');
                    out.push_str(k);
                } else {
                    out.push_str("[\"");
                    out.push_str(&k.replace('\\', "\\\\").replace('"', "\\\""));
                    out.push_str("\"]");
                }
                cur = v;
            }
            serde_json::Value::Array(arr) => {
                let v = arr.get(idx)?;
                out.push('[');
                out.push_str(&idx.to_string());
                out.push(']');
                cur = v;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// 目标节点的「复制值」文本：标量出原值（字符串去引号），容器复用
/// `JsonService::format` 美化（沿用当前缩进 / 排序选项）。路径非法 => None。
pub fn node_copy_text(
    root: &serde_json::Value,
    positional_path: &str,
    options: &FormatOptions,
) -> Option<String> {
    let node = node_at_path(root, positional_path)?;
    Some(match node {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        container @ (serde_json::Value::Object(_) | serde_json::Value::Array(_)) => {
            let compact = serde_json::to_string(container).ok()?;
            JsonService::format(&compact, options).unwrap_or(compact)
        }
    })
}
