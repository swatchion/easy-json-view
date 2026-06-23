use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eazy_json_view::services::{JsonService, ValidationResult, FormatOptions};

fn generate_large_json(size: usize) -> String {
    let mut json = String::from("{");
    for i in 0..size {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(r#""key{}": {{"nested": "value{}", "number": {}, "array": [1, 2, 3]}}"#, i, i, i));
    }
    json.push('}');
    json
}

fn benchmark_validation(c: &mut Criterion) {
    let small_json = generate_large_json(10);
    let medium_json = generate_large_json(100);
    let large_json = generate_large_json(1000);

    c.bench_function("validate_small_json", |b| {
        b.iter(|| JsonService::validate(black_box(&small_json)))
    });

    c.bench_function("validate_medium_json", |b| {
        b.iter(|| JsonService::validate(black_box(&medium_json)))
    });

    c.bench_function("validate_large_json", |b| {
        b.iter(|| JsonService::validate(black_box(&large_json)))
    });
}

fn benchmark_formatting(c: &mut Criterion) {
    let small_json = generate_large_json(10);
    let medium_json = generate_large_json(100);
    let large_json = generate_large_json(1000);
    
    let options = FormatOptions { indent_size: 2, sort_keys: false };

    c.bench_function("format_small_json", |b| {
        b.iter(|| JsonService::format(black_box(&small_json), black_box(&options)))
    });

    c.bench_function("format_medium_json", |b| {
        b.iter(|| JsonService::format(black_box(&medium_json), black_box(&options)))
    });

    c.bench_function("format_large_json", |b| {
        b.iter(|| JsonService::format(black_box(&large_json), black_box(&options)))
    });
}

fn benchmark_minify(c: &mut Criterion) {
    let formatted_json = r#"{
        "name": "test",
        "value": 123,
        "nested": {
            "key": "value",
            "array": [
                1,
                2,
                3
            ]
        }
    }"#;

    c.bench_function("minify_json", |b| {
        b.iter(|| JsonService::minify(black_box(formatted_json)))
    });
}

fn benchmark_stats(c: &mut Criterion) {
    let complex_json = generate_large_json(100);

    c.bench_function("get_stats", |b| {
        b.iter(|| JsonService::get_stats(black_box(&complex_json)))
    });
}

criterion_group!(
    benches,
    benchmark_validation,
    benchmark_formatting,
    benchmark_minify,
    benchmark_stats
);
criterion_main!(benches);
