use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion};
use crux_core::config::types::{FilterConfig, ReplaceRule};
use crux_core::filter;

// ---------------------------------------------------------------------------
// Fixture data
// ---------------------------------------------------------------------------

fn git_status_input() -> &'static str {
    concat!(
        "On branch feature/add-oauth-provider\n",
        "Your branch is ahead of 'origin/feature/add-oauth-provider' by 2 commits.\n",
        "  (use \"git push\" to publish your local commits)\n",
        "\n",
        "Changes to be committed:\n",
        "  (use \"git restore --staged <file>...\" to unstage)\n",
        "\tM  src/auth/oauth.rs\n",
        "\tA  src/auth/providers/github.rs\n",
        "\tA  src/auth/providers/mod.rs\n",
        "\n",
        "Changes not staged for commit:\n",
        "  (use \"git add <file>...\" to update what will be committed)\n",
        "  (use \"git restore <file>...\" to discard changes in working directory)\n",
        "\tM  src/config.rs\n",
        "\tM  src/lib.rs\n",
        "\tM  tests/integration/auth_test.rs\n",
        "\n",
        "Untracked files:\n",
        "  (use \"git add <file>...\" to include in what will be committed)\n",
        "\t?? .env.local\n",
        "\t?? src/auth/providers/google.rs\n",
        "\t?? tests/fixtures/oauth_response.json\n",
        "\t?? tmp/debug.log\n",
    )
}

fn cargo_test_large_input() -> String {
    let mut lines = Vec::with_capacity(200);
    lines.push("   Compiling crux-core v0.2.0".to_string());
    lines.push("   Compiling crux-cli v0.2.0".to_string());
    lines.push(
        "    Finished `test` profile [unoptimized + debuginfo] target(s) in 8.31s".to_string(),
    );
    lines.push("     Running unittests src/lib.rs".to_string());
    lines.push(String::new());
    lines.push("running 100 tests".to_string());
    for i in 0..100 {
        let status = if i % 20 == 7 { "FAILED" } else { "ok" };
        lines.push(format!("test module::tests::test_case_{i:03} ... {status}"));
    }
    lines.push(String::new());
    lines.push("failures:".to_string());
    lines.push(String::new());
    for i in (0..100).filter(|i| i % 20 == 7) {
        lines.push(format!("---- module::tests::test_case_{i:03} ----"));
        lines.push(format!(
            "thread 'module::tests::test_case_{i:03}' panicked at src/lib.rs:{}:9:",
            100 + i
        ));
        lines.push("assertion `left == right` failed".to_string());
        lines.push(format!("  left: \"expected_{i}\""));
        lines.push(format!("  right: \"actual_{i}\""));
        lines.push(String::new());
    }
    lines.push("failures:".to_string());
    for i in (0..100).filter(|i| i % 20 == 7) {
        lines.push(format!("    module::tests::test_case_{i:03}"));
    }
    lines.push(String::new());
    lines.push(
        "test result: FAILED. 95 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s".to_string(),
    );
    lines.push(String::new());
    lines.push("error: test failed, to rerun pass `--lib`".to_string());
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// apply_filter benchmarks
// ---------------------------------------------------------------------------

fn bench_apply_filter_git_status(c: &mut Criterion) {
    let config = FilterConfig {
        command: "git status".to_string(),
        ..Default::default()
    };
    let input = git_status_input();

    c.bench_function("apply_filter/git_status", |b: &mut Bencher| {
        b.iter(|| filter::apply_filter(black_box(&config), black_box(input), 0))
    });
}

fn bench_apply_filter_cargo_test(c: &mut Criterion) {
    let config = FilterConfig {
        command: "cargo test".to_string(),
        ..Default::default()
    };
    let input = cargo_test_large_input();

    c.bench_function("apply_filter/cargo_test_large", |b: &mut Bencher| {
        b.iter(|| filter::apply_filter(black_box(&config), black_box(&input), 1))
    });
}

// ---------------------------------------------------------------------------
// resolve_filter benchmark
// ---------------------------------------------------------------------------

fn bench_resolve_filter(c: &mut Criterion) {
    // Warm up the builtin registry
    let _ = filter::builtin::registry();

    let commands: Vec<Vec<String>> = vec![
        vec!["git".into(), "status".into()],
        vec!["cargo".into(), "test".into()],
        vec!["npm".into(), "install".into()],
        vec!["unknown-tool".into(), "subcommand".into()],
    ];

    c.bench_function("resolve_filter/git_status", |b: &mut Bencher| {
        b.iter(|| crux_core::config::resolve_filter(black_box(&commands[0])))
    });

    c.bench_function("resolve_filter/cargo_test", |b: &mut Bencher| {
        b.iter(|| crux_core::config::resolve_filter(black_box(&commands[1])))
    });

    c.bench_function("resolve_filter/unknown", |b: &mut Bencher| {
        b.iter(|| crux_core::config::resolve_filter(black_box(&commands[3])))
    });
}

// ---------------------------------------------------------------------------
// TOML pipeline stage benchmarks
// ---------------------------------------------------------------------------

fn bench_skip_stage(c: &mut Criterion) {
    let input = (0..500)
        .map(|i| {
            if i % 5 == 0 {
                format!("# comment line {i}")
            } else {
                format!("data line {i}: value={}", i * 42)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let skip_patterns = vec!["^#".to_string(), "^\\s*$".to_string()];

    c.bench_function("stage/skip_500_lines", |b: &mut Bencher| {
        b.iter(|| filter::skip::apply_skip_keep(black_box(&input), black_box(&skip_patterns), &[]))
    });
}

fn bench_replace_stage(c: &mut Criterion) {
    let input = (0..200)
        .map(|i| {
            format!(
                "2024-01-{:02} timestamp=1706{:06} msg=event_{i}",
                i % 28 + 1,
                i
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let rules = vec![
        ReplaceRule {
            pattern: r"\d{4}-\d{2}-\d{2}".to_string(),
            replacement: "DATE".to_string(),
        },
        ReplaceRule {
            pattern: r"timestamp=\d+".to_string(),
            replacement: "timestamp=X".to_string(),
        },
    ];

    c.bench_function("stage/replace_200_lines", |b: &mut Bencher| {
        b.iter(|| filter::replace::apply_replace(black_box(&input), black_box(&rules)))
    });
}

fn bench_dedup_stage(c: &mut Criterion) {
    let input = (0..1000)
        .map(|i| format!("line {}", i / 5)) // creates groups of 5 duplicates
        .collect::<Vec<_>>()
        .join("\n");

    c.bench_function("stage/dedup_1000_lines", |b: &mut Bencher| {
        b.iter(|| filter::dedup::apply_dedup(black_box(&input)))
    });
}

// ---------------------------------------------------------------------------
// Builtin registry benchmark
// ---------------------------------------------------------------------------

fn bench_builtin_registry_lookup(c: &mut Criterion) {
    // Ensure registry is initialized before benchmarking lookup
    let _ = filter::builtin::registry();

    c.bench_function("builtin/registry_lookup_hit", |b: &mut Bencher| {
        b.iter(|| filter::builtin::registry().get(black_box("git status")))
    });

    c.bench_function("builtin/registry_lookup_miss", |b: &mut Bencher| {
        b.iter(|| filter::builtin::registry().get(black_box("nonexistent command")))
    });
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

criterion_group!(
    filter_benches,
    bench_apply_filter_git_status,
    bench_apply_filter_cargo_test,
    bench_resolve_filter,
    bench_skip_stage,
    bench_replace_stage,
    bench_dedup_stage,
    bench_builtin_registry_lookup,
);
criterion_main!(filter_benches);
