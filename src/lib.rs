// EazyJsonView 库入口：导出纯逻辑 services 供 benches/ 与单元测试复用。
// 单元测试挂在 lib 这一侧，使 `cargo test --lib` 不经由 bin 入口、不拉 UI。

mod platform;

pub mod services {
    pub use crate::services_impl::*;
}

#[path = "services/mod_enhanced.rs"]
mod services_impl;

#[cfg(test)]
mod tests;
