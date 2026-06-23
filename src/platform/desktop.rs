//! 桌面平台 Storage：把所有 KV 持久化到用户配置目录下的单个 `store.json`。
//!
//! 形如 `~/.config/easy-json-view/store.json`，内部是一个 `{ key: value }` 映射，
//! `easy_json_view_history` / `easy_json_view_config` 各占一个键——天然满足「单文件」要求。
//!
//! 进程内用 `Mutex<Option<HashMap>>` 做惰性缓存：首次访问读盘，之后读写走内存，
//! `set` / `delete` 改动后整体回写（写前 `create_dir_all`，`to_string_pretty` 落盘）。

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// 进程内缓存。`None` 表示尚未从磁盘加载。
static CACHE: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);

/// `store.json` 的完整路径：<config_dir>/easy-json-view/store.json
fn store_path() -> Result<PathBuf> {
    let mut dir = dirs::config_dir().context("无法定位用户配置目录")?;
    dir.push("easy-json-view");
    Ok(dir.join("store.json"))
}

/// 从磁盘读入整张映射；文件缺失/损坏一律回退空映射（与「首次启动无配置」语义一致）。
fn load_from_disk() -> HashMap<String, Value> {
    let Ok(path) = store_path() else {
        return HashMap::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 确保缓存已加载，返回其可变引用。
fn loaded(slot: &mut Option<HashMap<String, Value>>) -> &mut HashMap<String, Value> {
    if slot.is_none() {
        *slot = Some(load_from_disk());
    }
    slot.as_mut().expect("缓存刚被填充")
}

/// 将整张映射写回磁盘（pretty）。写前确保父目录存在。
fn persist(map: &HashMap<String, Value>) -> Result<()> {
    let path = store_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("创建配置目录失败")?;
    }
    let text = serde_json::to_string_pretty(map).context("序列化 store.json 失败")?;
    std::fs::write(&path, text).context("写入 store.json 失败")?;
    Ok(())
}

pub struct Storage;

impl Storage {
    /// 读取并反序列化 `key` 对应的值；键缺失或解析失败均返回 `Err`。
    pub fn get<T: serde::de::DeserializeOwned>(key: &str) -> Result<T> {
        let mut guard = CACHE.lock().expect("store 缓存锁中毒");
        let map = loaded(&mut guard);
        let value = map.get(key).context("键不存在")?;
        serde_json::from_value(value.clone()).context("反序列化 store 值失败")
    }

    /// 序列化并写入 `key`，随后整体回写磁盘。
    pub fn set<T: serde::Serialize>(key: &str, value: &T) -> Result<()> {
        let mut guard = CACHE.lock().expect("store 缓存锁中毒");
        let map = loaded(&mut guard);
        map.insert(key.to_string(), serde_json::to_value(value)?);
        persist(map)
    }

    /// 删除 `key`（不存在时静默）；与 Web 实现对齐，落盘失败也不返回错误。
    pub fn delete(key: &str) {
        let mut guard = CACHE.lock().expect("store 缓存锁中毒");
        let map = loaded(&mut guard);
        if map.remove(key).is_some() {
            let _ = persist(map);
        }
    }
}
