//! 桌面持久化（store.json）端到端验证。
//!
//! 经**公开的** `HistoryService` / `ConfigService`（其内部走 `crate::platform::Storage`，
//! 在非 wasm 目标上即 `platform::desktop` 的单文件实现）验证：写盘、回读、单文件双键。
//! 这条路径只用到 lib，不依赖桌面 renderer，故可在 `--no-default-features` 下运行，
//! 无需 webkit/libxdo、无需 GUI。
//!
//! 用临时 `XDG_CONFIG_HOME` 隔离配置目录（`dirs::config_dir()` 在 Linux 上遵循它），
//! 避免污染真实 `~/.config/easy-json-view/store.json`。
#![cfg(not(target_arch = "wasm32"))]

use easy_json_view::services::{AppConfig, ConfigService, HistoryRecord, HistoryService};

#[test]
fn desktop_store_json_roundtrip() {
    // 隔离到进程私有的临时配置目录
    let tmp = std::env::temp_dir().join(format!("ejv-store-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", &tmp);

    tokio_test::block_on(async {
        // 首次：无文件 → 空历史
        assert!(HistoryService::load_history().await.unwrap().is_empty());

        // 写一条历史与一份配置 → 应创建 store.json
        let rec = HistoryRecord::new("{\"a\":1}".to_string(), "{\n  \"a\": 1\n}".to_string());
        HistoryService::save_record(&rec).await.unwrap();
        ConfigService::save_config(&AppConfig::default()).await.unwrap();

        // store.json 存在，且为「单文件 + 两个键」
        let store = tmp.join("easy-json-view").join("store.json");
        assert!(store.exists(), "store.json 应已落盘");
        let text = std::fs::read_to_string(&store).unwrap();
        assert!(text.contains("easy_json_view_history"), "应含 history 键");
        assert!(text.contains("easy_json_view_config"), "应含 config 键");

        // 回读一致
        let back = HistoryService::load_history().await.unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].content, "{\"a\":1}");

        // 删除后落盘仍一致（键被移除但文件保留）
        HistoryService::delete_record(&rec.id).await.unwrap();
        assert!(HistoryService::load_history().await.unwrap().is_empty());
    });

    let _ = std::fs::remove_dir_all(&tmp);
}
