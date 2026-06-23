//! Web 平台 Storage：直接转发到浏览器 `localStorage`（gloo_storage）。
//! 把 gloo 的 `StorageError` 统一收敛为 `anyhow::Error`，使签名与桌面实现对齐。

use anyhow::Result;
use gloo_storage::{LocalStorage, Storage as _};

pub struct Storage;

impl Storage {
    /// 读取并反序列化 `key` 对应的值；键缺失或解析失败均返回 `Err`。
    pub fn get<T: serde::de::DeserializeOwned>(key: &str) -> Result<T> {
        LocalStorage::get::<T>(key).map_err(|e| anyhow::anyhow!("{:?}", e))
    }

    /// 序列化并写入 `key`。
    pub fn set<T: serde::Serialize>(key: &str, value: &T) -> Result<()> {
        LocalStorage::set(key, value).map_err(|e| anyhow::anyhow!("{:?}", e))
    }

    /// 删除 `key`（不存在时静默）。
    pub fn delete(key: &str) {
        LocalStorage::delete(key);
    }
}
