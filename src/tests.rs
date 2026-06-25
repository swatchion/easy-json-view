#[cfg(test)]
mod tests {
    use crate::services::{JsonService, ValidationResult, ValidationErrorKind, FormatOptions, HistoryRecord};
    use crate::services::{build_tree_rows, collect_container_paths, path_to_expr, node_copy_text};
    use crate::services::set_record_formatted;
    use crate::services::{collect_search_expansions, find_matches};
    use crate::services::{AppConfig, UiSettings};
    use std::collections::HashSet;

    fn paths(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_collect_container_paths_by_depth() {
        // 结构：root(0) -> a:{b}(1) / c:[{d}](1) / e:5
        //   ".0"   = a 的对象（深度1）
        //   ".1"   = c 的数组（深度1）
        //   ".1.0" = 数组内的 {d} 对象（深度2）
        // e 是标量，不计入容器
        let v: serde_json::Value =
            serde_json::from_str(r#"{"a":{"b":1},"c":[{"d":2}],"e":5}"#).unwrap();

        // 折叠全部：深度 >= 1
        assert_eq!(collect_container_paths(&v, 1), paths(&[".0", ".1", ".1.0"]));
        // 折叠全部二级字段：深度 >= 2
        assert_eq!(collect_container_paths(&v, 2), paths(&[".1.0"]));
        // 全部展开对应空集（深度 >= 一个很大的值）
        assert!(collect_container_paths(&v, 99).is_empty());
    }

    #[test]
    fn test_collect_container_paths_ignores_empty_containers() {
        // 空 {} / [] 不可折叠，不应出现在折叠集合中
        let v: serde_json::Value = serde_json::from_str(r#"{"a":{},"b":[]}"#).unwrap();
        assert_eq!(collect_container_paths(&v, 1), HashSet::new());
    }

    #[test]
    fn test_build_tree_rows_expanded_and_empty_literal() {
        let v: serde_json::Value = serde_json::from_str(r#"{"a":{"b":1},"c":{}}"#).unwrap();
        let rows = build_tree_rows(&v, &HashSet::new(), 1000);

        // 行序：root{ , a{ , b:1 , a的} , c:{}（空对象字面量） , root的}
        assert_eq!(rows.len(), 6);
        assert!(rows[0].is_container && !rows[0].collapsed); // root
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[1].key_label.as_deref(), Some("a"));
        assert!(rows[1].is_container);
        assert_eq!(rows[2].key_label.as_deref(), Some("b"));
        assert!(!rows[2].is_container);
        assert_eq!(rows[2].value_text, "1");
        // a 的闭括号行（depth 1）
        assert!(rows[3].is_close && !rows[3].is_container);
        assert_eq!(rows[3].value_text, "}");
        assert_eq!(rows[3].depth, 1);
        // 空对象是字面量行，不可折叠
        assert_eq!(rows[4].key_label.as_deref(), Some("c"));
        assert!(!rows[4].is_container);
        assert_eq!(rows[4].value_text, "{}");
        // root 的闭括号行（depth 0）
        assert!(rows[5].is_close);
        assert_eq!(rows[5].value_text, "}");
        assert_eq!(rows[5].depth, 0);
    }

    #[test]
    fn test_build_tree_rows_collapsed_summary_does_not_recurse() {
        let v: serde_json::Value = serde_json::from_str(r#"{"a":{"b":1},"c":{"d":2}}"#).unwrap();
        // 折叠 root（path = ""）
        let mut collapsed = HashSet::new();
        collapsed.insert(String::new());
        let rows = build_tree_rows(&v, &collapsed, 1000);

        assert_eq!(rows.len(), 1); // 折叠后只剩一行摘要，不递归子节点
        assert!(rows[0].collapsed);
        assert_eq!(rows[0].item_count, 2);
        // 新模型：容器 value_text 仅放开括号；折叠摘要由 collapsed_preview + item_count 组装
        assert_eq!(rows[0].value_text, "{");
        assert_eq!(rows[0].collapsed_preview.as_deref(), Some("a, c"));
    }

    #[test]
    fn test_collapsed_preview_object_truncates_at_four_keys() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"a":1,"b":2,"c":3,"d":4,"e":5,"f":6}"#).unwrap();
        let mut collapsed = HashSet::new();
        collapsed.insert(String::new()); // 折叠 root
        let rows = build_tree_rows(&v, &collapsed, 1000);
        assert_eq!(rows.len(), 1);
        // 首 4 键 + "…"
        assert_eq!(rows[0].collapsed_preview.as_deref(), Some("a, b, c, d, …"));
    }

    #[test]
    fn test_collapsed_preview_array_is_none() {
        let v: serde_json::Value = serde_json::from_str(r#"[1,2,3]"#).unwrap();
        let mut collapsed = HashSet::new();
        collapsed.insert(String::new());
        let rows = build_tree_rows(&v, &collapsed, 1000);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].collapsed);
        assert_eq!(rows[0].value_text, "[");
        assert!(rows[0].collapsed_preview.is_none()); // 数组不预览键
    }

    // 路径回溯：{"data":{"items":[{"id":7,"weird key":1}]}}
    //   ".0"        = data 对象
    //   ".0.0"      = items 数组
    //   ".0.0.0"    = items[0] 对象
    //   ".0.0.0.0"  = id（标识符键 → .id）
    //   ".0.0.0.1"  = "weird key"（含空格 → ["weird key"]）
    const EXPR_DOC: &str = r#"{"data":{"items":[{"id":7,"weird key":1}]}}"#;

    #[test]
    fn test_path_to_expr_jsonpath() {
        let v: serde_json::Value = serde_json::from_str(EXPR_DOC).unwrap();
        assert_eq!(path_to_expr(&v, "").as_deref(), Some("$"));
        assert_eq!(path_to_expr(&v, ".0").as_deref(), Some("$.data"));
        assert_eq!(path_to_expr(&v, ".0.0").as_deref(), Some("$.data.items"));
        assert_eq!(path_to_expr(&v, ".0.0.0").as_deref(), Some("$.data.items[0]"));
        assert_eq!(path_to_expr(&v, ".0.0.0.0").as_deref(), Some("$.data.items[0].id"));
        assert_eq!(
            path_to_expr(&v, ".0.0.0.1").as_deref(),
            Some(r#"$.data.items[0]["weird key"]"#)
        );
        // 越界路径 => None
        assert!(path_to_expr(&v, ".9").is_none());
    }

    #[test]
    fn test_node_copy_text_scalars_and_containers() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"name":"Alice","nums":[1,2],"obj":{"k":true}}"#).unwrap();
        let opts = FormatOptions { indent_size: 2, sort_keys: false };
        // 字符串：去引号出原值
        assert_eq!(node_copy_text(&v, ".0", &opts).as_deref(), Some("Alice"));
        // 布尔标量
        assert_eq!(node_copy_text(&v, ".2.0", &opts).as_deref(), Some("true"));
        // 容器：美化（按缩进 2）
        assert_eq!(node_copy_text(&v, ".1", &opts).as_deref(), Some("[\n  1,\n  2\n]"));
        // 越界 => None
        assert!(node_copy_text(&v, ".9", &opts).is_none());
    }

    #[test]
    fn test_build_tree_rows_scalar_root() {
        let v: serde_json::Value = serde_json::from_str("42").unwrap();
        let rows = build_tree_rows(&v, &HashSet::new(), 1000);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].is_container);
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[0].value_text, "42");
        // 标量根没有任何可折叠容器
        assert!(collect_container_paths(&v, 1).is_empty());
    }

    #[test]
    fn test_build_tree_rows_node_cap() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"a":1,"b":2,"c":3,"d":4,"e":5}"#).unwrap();
        // cap=3 时最多产出 3 行（root{ + 两个成员）
        let rows = build_tree_rows(&v, &HashSet::new(), 3);
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn test_build_tree_rows_closing_brackets_expanded() {
        // {"a":1,"b":[2,3]} 展开后应产出闭括号行：{ a [ 2 3 ] }
        let v: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":[2,3]}"#).unwrap();
        let rows = build_tree_rows(&v, &HashSet::new(), 1000);
        assert_eq!(rows.len(), 7); // 5 内容行 + 2 闭括号行（] 和 }）
        // root 开括号
        assert!(rows[0].is_container && !rows[0].is_close);
        assert_eq!(rows[0].value_text, "{");
        // b 数组开括号
        assert_eq!(rows[2].key_label.as_deref(), Some("b"));
        assert_eq!(rows[2].value_text, "[");
        // b 的闭括号行：depth 1、is_close、无键、非容器
        assert!(rows[5].is_close && !rows[5].is_container);
        assert_eq!(rows[5].value_text, "]");
        assert_eq!(rows[5].depth, 1);
        assert!(rows[5].key_label.is_none());
        // root 闭括号行：depth 0
        assert!(rows[6].is_close);
        assert_eq!(rows[6].value_text, "}");
        assert_eq!(rows[6].depth, 0);
    }

    #[test]
    fn test_build_tree_rows_trailing_commas() {
        // {"a":1,"b":[2,3]} -> 行: 0{ 1 a:1 2 b:[ 3 :2 4 :3 5 ] 6 }
        let v: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":[2,3]}"#).unwrap();
        let rows = build_tree_rows(&v, &HashSet::new(), 1000);
        assert_eq!(rows.len(), 7);
        assert!(!rows[0].trailing_comma); // root 开括号无逗号
        assert!(rows[1].trailing_comma); // a 非末项 -> 逗号
        assert!(!rows[2].trailing_comma); // b 开括号无逗号（逗号落在其闭括号行）
        assert!(rows[3].trailing_comma); // 数组元素 2 非末项 -> 逗号
        assert!(!rows[4].trailing_comma); // 数组元素 3 末项 -> 无逗号
        assert!(!rows[5].trailing_comma); // b 的 ] ：b 是 root 末项 -> 无逗号
        assert!(!rows[6].trailing_comma); // root 的 } 无逗号
    }

    #[test]
    fn test_build_tree_rows_comma_after_container_and_collapsed() {
        // 容器非末项 -> 其闭括号带逗号：{"a":[1],"b":2}
        let v: serde_json::Value = serde_json::from_str(r#"{"a":[1],"b":2}"#).unwrap();
        let rows = build_tree_rows(&v, &HashSet::new(), 1000);
        // 0{ 1 a:[ 2 :1 3 ](a 的闭, a 非末项->逗号) 4 b:2(末项无逗号) 5 }(root)
        assert_eq!(rows.len(), 6);
        assert!(rows[3].is_close && rows[3].value_text == "]");
        assert!(rows[3].trailing_comma); // a 非末项 -> "],"
        assert!(!rows[4].trailing_comma); // b 末项标量 -> 无逗号

        // 折叠容器非末项 -> 摘要行带逗号：{"a":{"x":1},"b":2}，折叠 a（path ".0"）
        let v2: serde_json::Value = serde_json::from_str(r#"{"a":{"x":1},"b":2}"#).unwrap();
        let mut collapsed = HashSet::new();
        collapsed.insert(".0".to_string());
        let rows2 = build_tree_rows(&v2, &collapsed, 1000);
        // 0{ 1 a:{(折叠, 非末项->逗号) 2 b:2(末项) 3 }(root)
        assert_eq!(rows2.len(), 4);
        assert!(rows2[1].collapsed && rows2[1].trailing_comma);
        assert!(!rows2[2].trailing_comma);
    }

    // 结构：{"a":{"b":1},"c":[{"deep":"hi"}],"e":5}
    //   ""     = root 对象
    //   ".0"   = a 的对象（含叶 b）
    //   ".1"   = c 的数组
    //   ".1.0" = 数组内 {deep} 对象
    const SEARCH_DOC: &str = r#"{"a":{"b":1},"c":[{"deep":"hi"}],"e":5}"#;

    #[test]
    fn test_collect_search_expansions_key_hit_collects_ancestors() {
        let v: serde_json::Value = serde_json::from_str(SEARCH_DOC).unwrap();
        // 键 "b" 命中（叶节点 .0.0）=> 收集其祖先容器 root("") 与 a 的对象(".0")
        assert_eq!(collect_search_expansions(&v, "b", false), paths(&["", ".0"]));
    }

    #[test]
    fn test_collect_search_expansions_deep_key_hit() {
        let v: serde_json::Value = serde_json::from_str(SEARCH_DOC).unwrap();
        // 键 "deep" 命中（.1.0.0）=> root("")、数组(".1")、{deep}对象(".1.0")
        assert_eq!(collect_search_expansions(&v, "deep", false), paths(&["", ".1", ".1.0"]));
    }

    #[test]
    fn test_collect_search_expansions_value_hit() {
        let v: serde_json::Value = serde_json::from_str(SEARCH_DOC).unwrap();
        // 标量值 5 命中（e 是 root 直接子级）=> 仅 root("")
        assert_eq!(collect_search_expansions(&v, "5", false), paths(&[""]));
        // 字符串值 "hi"（scalar_repr 含引号）命中（.1.0.0 的值）=> root、数组、{deep}对象
        assert_eq!(collect_search_expansions(&v, "hi", false), paths(&["", ".1", ".1.0"]));
    }

    #[test]
    fn test_collect_search_expansions_container_key_only_not_self() {
        // 容器按键命中，仅收集祖先，不收集容器自身 path（容器无需展开自身即可显示其行）
        let v: serde_json::Value = serde_json::from_str(r#"{"obj":{"x":1}}"#).unwrap();
        let got = collect_search_expansions(&v, "obj", false);
        assert_eq!(got, paths(&[""])); // 仅 root；不含 ".0"（obj 容器自身）
        assert!(!got.contains(".0"));
    }

    #[test]
    fn test_collect_search_expansions_empty_and_miss() {
        let v: serde_json::Value = serde_json::from_str(SEARCH_DOC).unwrap();
        assert!(collect_search_expansions(&v, "", false).is_empty()); // 空查询
        assert!(collect_search_expansions(&v, "zzz", false).is_empty()); // 无命中
    }

    #[test]
    fn test_collect_search_expansions_case_insensitive() {
        let v: serde_json::Value = serde_json::from_str(SEARCH_DOC).unwrap();
        // cs=false：大写查询应命中小写键
        assert_eq!(collect_search_expansions(&v, "DEEP", false), paths(&["", ".1", ".1.0"]));
        // cs=true：大写查询不命中
        assert!(collect_search_expansions(&v, "DEEP", true).is_empty());
    }

    #[test]
    fn test_find_matches_basic_and_case() {
        // find_matches 已移入 services；保留一条基本断言确认行为不变
        assert_eq!(find_matches("aXaXa", "x", false), vec![(1, 2), (3, 4)]);
        assert!(find_matches("aXaXa", "x", true).is_empty());
        assert!(find_matches("abc", "", false).is_empty());
    }

    #[test]
    fn test_json_validation_valid() {
        let json = r#"{"name": "test", "value": 123}"#;
        match JsonService::validate(json) {
            ValidationResult::Valid => assert!(true),
            ValidationResult::Invalid { .. } => assert!(false, "Valid JSON should pass validation"),
        }
    }

    #[test]
    fn test_json_validation_invalid() {
        let json = r#"{"name": "test", "value": 123"#; // Missing closing brace
        match JsonService::validate(json) {
            ValidationResult::Valid => assert!(false, "Invalid JSON should fail validation"),
            ValidationResult::Invalid { .. } => assert!(true),
        }
    }

    #[test]
    fn test_json_validation_empty() {
        let json = "";
        match JsonService::validate(json) {
            ValidationResult::Valid => assert!(false, "Empty JSON should fail validation"),
            // 断言结构化的 kind 而非中文串：文案改了测试也不挂（service 语言无关的回报）
            ValidationResult::Invalid { kind, .. } => assert!(matches!(kind, ValidationErrorKind::Empty)),
        }
    }

    #[test]
    fn test_json_formatting() {
        let json = r#"{"name":"test","value":123,"nested":{"key":"value"}}"#;
        let options = FormatOptions { indent_size: 2, sort_keys: false };
        
        let result = JsonService::format(json, &options);
        assert!(result.is_ok());
        
        let formatted = result.unwrap();
        assert!(formatted.contains("  ")); // Should contain indentation
        assert!(formatted.lines().count() > 1); // Should be multi-line
    }

    #[test]
    fn test_json_formatting_different_indents() {
        let json = r#"{"name":"test","value":123}"#;
        
        for indent_size in [2, 4, 8] {
            let options = FormatOptions { indent_size, sort_keys: false };
            let result = JsonService::format(json, &options);
            assert!(result.is_ok());
            
            let formatted = result.unwrap();
            let indent = " ".repeat(indent_size);
            assert!(formatted.contains(&indent));
        }
    }

    #[test]
    fn test_json_minify() {
        let json = r#"{
            "name": "test",
            "value": 123,
            "nested": {
                "key": "value"
            }
        }"#;
        
        let result = JsonService::minify(json);
        assert!(result.is_ok());
        
        let minified = result.unwrap();
        assert!(!minified.contains("  ")); // Should not contain extra spaces
        assert!(!minified.contains("\n")); // Should not contain newlines
    }

    #[test]
    fn test_json_stats() {
        let json = r#"{
            "name": "test",
            "value": 123,
            "active": true,
            "data": null,
            "items": [1, 2, 3],
            "nested": {
                "key": "value"
            }
        }"#;
        
        let result = JsonService::get_stats(json);
        assert!(result.is_ok());
        
        let stats = result.unwrap();
        assert_eq!(stats.objects, 2); // Root object + nested object
        assert_eq!(stats.arrays, 1);  // items array
        assert_eq!(stats.strings, 2); // "test", "value" (keys are counted separately)
        assert_eq!(stats.numbers, 4); // 123, 1, 2, 3
        assert_eq!(stats.booleans, 1); // true
        assert_eq!(stats.nulls, 1);   // null
    }

    #[test]
    fn test_large_json_performance() {
        // Create a large JSON object
        let mut large_json = String::from("{");
        for i in 0..1000 {
            if i > 0 {
                large_json.push(',');
            }
            large_json.push_str(&format!(r#""key{}": "value{}""#, i, i));
        }
        large_json.push('}');

        let start = std::time::Instant::now();
        let result = JsonService::validate(&large_json);
        let duration = start.elapsed();

        assert!(matches!(result, ValidationResult::Valid));
        assert!(duration.as_millis() < 100, "Validation should be fast for large JSON");

        let start = std::time::Instant::now();
        let format_result = JsonService::format(&large_json, &FormatOptions { indent_size: 2, sort_keys: false });
        let duration = start.elapsed();

        assert!(format_result.is_ok());
        assert!(duration.as_millis() < 500, "Formatting should be reasonably fast for large JSON");
    }

    #[test]
    fn test_malformed_json_edge_cases() {
        let test_cases = vec![
            (r#"{"#, "incomplete object"),
            (r#"[1,2,3"#, "incomplete array"),
            (r#"{"key": }"#, "missing value"),
            (r#"{"key" "value"}"#, "missing colon"),
            (r#"{key: "value"}"#, "unquoted key"),
            (r#"{"key": 'value'}"#, "single quotes"),
        ];

        for (json, description) in test_cases {
            match JsonService::validate(json) {
                ValidationResult::Valid => assert!(false, "Should fail for {}", description),
                ValidationResult::Invalid { .. } => assert!(true),
            }
        }
    }

    #[test]
    fn test_unicode_json() {
        let json = r#"{"中文": "测试", "emoji": "🎉", "unicode": "\u0048\u0065\u006C\u006C\u006F"}"#;
        
        match JsonService::validate(json) {
            ValidationResult::Valid => assert!(true),
            ValidationResult::Invalid { kind, .. } => assert!(false, "Unicode JSON should be valid: {:?}", kind),
        }

        let result = JsonService::format(json, &FormatOptions { indent_size: 2, sort_keys: false });
        assert!(result.is_ok());
        
        let formatted = result.unwrap();
        assert!(formatted.contains("中文"));
        assert!(formatted.contains("🎉"));
    }

    #[test]
    fn test_history_record_creation() {
        let content = r#"{"test": "value"}"#.to_string();
        let formatted = r#"{
  "test": "value"
}"#.to_string();

        let record = HistoryRecord::new(content.clone(), formatted.clone());

        // 验证记录创建
        assert_eq!(record.content, content);
        assert_eq!(record.formatted_content, formatted);
        assert!(!record.id.is_empty());
        assert!(!record.name.is_empty());
        assert!(!record.created_at.is_empty());

        // 验证名称是 Git 风格的短 hash（7个字符的十六进制）
        assert_eq!(record.name.len(), 7);
        assert!(record.name.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_history_record_helpers() {
        let content = r#"   {"test": "value with some long content that should be truncated"}   "#.to_string();
        let formatted = r#"{
  "test": "value"
}"#.to_string();

        let record = HistoryRecord::new(content, formatted);

        // 测试格式化时间
        let formatted_time = record.formatted_created_at();
        assert!(formatted_time.contains("-")); // 应该包含日期分隔符
        assert!(formatted_time.contains(":")); // 应该包含时间分隔符

        // 测试内容预览
        let preview = record.content_preview();
        assert!(!preview.starts_with(" ")); // 应该被 trim
        assert!(!preview.ends_with(" ")); // 应该被 trim
        assert!(preview.len() <= 53); // 最多50个字符 + "..."
    }

    #[test]
    fn test_history_record_hash_consistency() {
        let content = r#"{"test": "value"}"#.to_string();
        let formatted = r#"{
  "test": "value"
}"#.to_string();

        // 创建两个相同内容的记录
        let record1 = HistoryRecord::new(content.clone(), formatted.clone());
        let record2 = HistoryRecord::new(content.clone(), formatted.clone());

        // 验证相同内容生成相同的 hash 名称
        assert_eq!(record1.name, record2.name);

        // 验证不同内容生成不同的 hash
        let different_content = r#"{"different": "content"}"#.to_string();
        let record3 = HistoryRecord::new(different_content, formatted);
        assert_ne!(record1.name, record3.name);
    }

    #[test]
    fn test_history_record_id_unique_same_content() {
        // 同一内容连续创建应得到不同 id（毫秒+计数器），修复秒级时间戳碰撞
        let content = r#"{"a":1}"#.to_string();
        let r1 = HistoryRecord::new(content.clone(), content.clone());
        let r2 = HistoryRecord::new(content.clone(), content.clone());
        assert_ne!(r1.id, r2.id, "相同内容也应生成不同的 id");
        assert_eq!(r1.name, r2.name, "相同内容名称（hash）应一致");
    }

    #[test]
    fn test_set_record_formatted_updates_only_target_field() {
        // set_record_formatted 为 update_record_formatted 的纯逻辑内核：
        // 仅改命中 id 的 formatted_content，其它字段与其它记录均不动。
        let mut records = vec![
            HistoryRecord::new(r#"{"a":1}"#.to_string(), "OLD_A".to_string()),
            HistoryRecord::new(r#"{"b":2}"#.to_string(), "OLD_B".to_string()),
        ];
        let r0 = records[0].clone();
        let r1_before = records[1].clone();

        let hit = set_record_formatted(&mut records, &r0.id, "NEW_A".to_string());
        assert!(hit, "命中已存 id 应返回 true");

        // 目标项：仅 formatted_content 变化
        assert_eq!(records[0].formatted_content, "NEW_A");
        assert_eq!(records[0].id, r0.id);
        assert_eq!(records[0].name, r0.name);
        assert_eq!(records[0].content, r0.content);
        assert_eq!(records[0].created_at, r0.created_at);
        assert_eq!(records[0].bookmarked, r0.bookmarked);

        // 其它项：完全不变
        assert_eq!(records[1], r1_before);

        // 未命中 id：返回 false 且整表不变
        let before = records.clone();
        assert!(!set_record_formatted(&mut records, "no-such-id", "X".to_string()));
        assert_eq!(records, before);
    }

    #[test]
    fn test_trimmed_inputs_share_dedup_key() {
        // 去重/匹配键 = record.content。trim 后首尾空白不同的输入应得到相等 content，
        // 故 app 层 history_records.find(|r| r.content == input) 能命中并复用原记录。
        let a = "  {\"a\":1}\n".trim().to_string();
        let b = "\n\t{\"a\":1}  ".trim().to_string();
        assert_eq!(a, b, "trim 后两输入应字节相等");

        let ra = HistoryRecord::new(a, "x".to_string());
        let rb = HistoryRecord::new(b, "y".to_string());
        assert_eq!(ra.content, rb.content, "content 相等 → 匹配/去重命中");
        assert_eq!(ra.name, rb.name, "内容相同 → 短 hash 名一致");
    }

    #[test]
    fn test_format_preserves_key_order_by_default() {
        let json = r#"{"banana":1,"apple":2,"cherry":3}"#;
        let out = JsonService::format(json, &FormatOptions { indent_size: 2, sort_keys: false }).unwrap();
        let b = out.find("banana").unwrap();
        let a = out.find("apple").unwrap();
        let c = out.find("cherry").unwrap();
        assert!(b < a && a < c, "默认应保留原始键序 banana,apple,cherry");
    }

    #[test]
    fn test_format_sort_keys() {
        let json = r#"{"banana":1,"apple":2,"cherry":3}"#;
        let out = JsonService::format(json, &FormatOptions { indent_size: 2, sort_keys: true }).unwrap();
        let a = out.find("apple").unwrap();
        let b = out.find("banana").unwrap();
        let c = out.find("cherry").unwrap();
        assert!(a < b && b < c, "开启排序后应为 apple,banana,cherry");
    }

    #[test]
    fn test_app_config_serde_round_trip() {
        // 完整 round-trip：两份偏好都应原样保留（确保 save_cfg 写入后再读不丢 ui_settings）
        let cfg = AppConfig {
            format_options: FormatOptions { indent_size: 8, sort_keys: true },
            ui_settings: UiSettings { theme: "dark".to_string(), font_size: 16, auto_format: true, language: "zh-CN".to_string(), density: "compact".to_string(), show_line_numbers: true },
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.format_options.indent_size, 8);
        assert!(back.format_options.sort_keys);
        assert_eq!(back.ui_settings.theme, "dark");
        assert_eq!(back.ui_settings.font_size, 16);
        assert!(back.ui_settings.auto_format);
        assert_eq!(back.ui_settings.language, "zh-CN");
        assert_eq!(back.ui_settings.density, "compact");
        assert!(back.ui_settings.show_line_numbers);
    }

    #[test]
    fn test_app_config_partial_deserializes_with_defaults() {
        // 缺失 ui_settings 的旧/部分 config：#[serde(default)] 应让其回退默认值而非整体解析失败
        let partial = r#"{"format_options":{"indent_size":2,"sort_keys":false}}"#;
        let cfg: AppConfig = serde_json::from_str(partial).unwrap();
        assert_eq!(cfg.format_options.indent_size, 2);
        assert_eq!(cfg.ui_settings.theme, "light"); // 来自 UiSettings::default()
        assert_eq!(cfg.ui_settings.font_size, 14);
        assert!(cfg.ui_settings.auto_format); // 默认 true（自动格式化默认开启）
        assert_eq!(cfg.ui_settings.language, "en"); // 旧 config 无 language 字段 → 回退默认英文
        assert_eq!(cfg.ui_settings.density, "comfortable"); // 旧 config 无 density 字段 → 回退默认舒适
        assert!(!cfg.ui_settings.show_line_numbers); // 旧 config 无此字段 → 回退默认 false（不显示行号）

        // 空对象也应整体回退默认，不 panic
        let empty: AppConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.format_options.indent_size, 4);
        assert_eq!(empty.ui_settings.font_size, 14);
        assert_eq!(empty.ui_settings.language, "en");
        assert_eq!(empty.ui_settings.density, "comfortable");
    }
}
