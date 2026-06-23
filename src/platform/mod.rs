//! 平台抽象层：按目标架构路由出同一形态的 `Storage` KV 垫片。
//!
//! - `wasm32`（Web）：转发 `gloo_storage::LocalStorage`，行为与历史版本完全一致。
//! - 其它（桌面/原生）：落盘到用户配置目录下的单个 `store.json`。
//!
//! 两个实现暴露同样的 `Storage::{get, set, delete}` 签名，上层 `services` 仅
//! `use crate::platform::Storage` 即可，无需关心底层是 localStorage 还是文件。

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::Storage;

#[cfg(not(target_arch = "wasm32"))]
mod desktop;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop::Storage;
