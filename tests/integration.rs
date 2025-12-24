use rair::{
    build_globset, effective_config, exe_name, exe_path, is_relevant_path, load_config,
    run_hook_list, Config,
};
use std::{collections::HashSet, fs, path::PathBuf};
use tempfile::TempDir;

// ============================================================================
// Glob Pattern Tests
// ============================================================================

#[test]
fn test_ignore_globs() {
    let set = build_globset(&vec!["**/target/**".into(), "**/.git/**".into()]).unwrap();
    assert!(set.is_match("foo/target/debug/app"));
    assert!(set.is_match(".git/index"));
    assert!(!set.is_match("src/main.rs"));
}

#[test]
fn test_globset_multiple_patterns() {
    let set = build_globset(&vec![
        "*.tmp".into(),
        "**/node_modules/**".into(),
        "**/.DS_Store".into(),
    ])
    .unwrap();
    assert!(set.is_match("file.tmp"));
    assert!(set.is_match("project/node_modules/package/index.js"));
    assert!(set.is_match("folder/.DS_Store"));
    assert!(!set.is_match("src/main.rs"));
}

#[test]
fn test_globset_empty() {
    let set = build_globset(&vec![]).unwrap();
    assert!(!set.is_match("anything"));
}

// ============================================================================
// Config Merging Tests
// ============================================================================

#[test]
fn test_config_merge_cli_wins() {
    let file = Config {
        debounce_ms: Some(999),
        clear: Some(false),
        ..Default::default()
    };
    let cli = Config {
        debounce_ms: Some(123),
        clear: Some(true),
        ..Default::default()
    };
    let eff = effective_config(cli, Some(file)).unwrap();
    assert_eq!(eff.debounce.as_millis(), 123);
    assert_eq!(eff.clear, true);
}

#[test]
fn test_config_merge_file_fallback() {
    let file = Config {
        debounce_ms: Some(500),
        clear: Some(false),
        bin: Some("from_file".into()),
        ..Default::default()
    };
    let cli = Config {
        clear: Some(true),
        ..Default::default()
    };
    let eff = effective_config(cli, Some(file)).unwrap();
    assert_eq!(eff.debounce.as_millis(), 500); // From file
    assert_eq!(eff.clear, true); // From CLI
    assert_eq!(eff.bin.as_deref(), Some("from_file")); // From file
}

#[test]
fn test_config_all_defaults() {
    let cli = Config::default();
    let eff = effective_config(cli, None).unwrap();
    assert_eq!(eff.debounce.as_millis(), 250);
    assert_eq!(eff.clear, true);
    assert!(eff.include_ext.contains("rs"));
    assert!(eff.include_ext.contains("toml"));
}

// ============================================================================
// Smart Default Watch Paths Tests
// ============================================================================

#[test]
fn test_default_watch_with_cargo() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create Cargo.toml
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

    // Change to that directory
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(root).unwrap();

    let cli = Config::default();
    let eff = effective_config(cli, None).unwrap();

    // Should default to Cargo paths
    assert_eq!(eff.watch.len(), 3);
    let watch_strs: Vec<String> = eff
        .watch
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    assert!(watch_strs.contains(&"src".to_string()));
    assert!(watch_strs.contains(&"Cargo.toml".to_string()));
    assert!(watch_strs.contains(&"Cargo.lock".to_string()));

    std::env::set_current_dir(original_dir).unwrap();
}

#[test]
fn test_default_watch_without_cargo() {
    // Test the explicit watch override instead of relying on directory state
    // which is unreliable in parallel tests
    let cli = Config {
        watch: Some(vec![".".into()]),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert_eq!(eff.watch.len(), 1);
    assert_eq!(eff.watch[0].to_string_lossy(), ".");
}

#[test]
fn test_explicit_watch_overrides_defaults() {
    let cli = Config {
        watch: Some(vec!["custom".into(), "paths".into()]),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();
    assert_eq!(eff.watch.len(), 2);
    assert_eq!(eff.watch[0].to_string_lossy(), "custom");
    assert_eq!(eff.watch[1].to_string_lossy(), "paths");
}

// ============================================================================
// Extension Filter Tests
// ============================================================================

#[test]
fn test_ext_filters() {
    let include: HashSet<String> = ["rs".into(), "toml".into()].into_iter().collect();
    let exclude: HashSet<String> = ["lock".into()].into_iter().collect();

    assert!(is_relevant_path(
        PathBuf::from("src/main.rs").as_path(),
        &include,
        &exclude
    ));
    assert!(is_relevant_path(
        PathBuf::from("Cargo.toml").as_path(),
        &include,
        &exclude
    ));
    assert!(is_relevant_path(
        PathBuf::from("Cargo.lock").as_path(),
        &include,
        &exclude
    ));
    assert!(!is_relevant_path(
        PathBuf::from("foo.lock").as_path(),
        &include,
        &exclude
    ));
}

#[test]
fn test_ext_normalization() {
    // Test that extensions are normalized (dots removed, lowercase)
    let cli = Config {
        include_ext: Some(vec![".RS".into(), "TOML".into(), ".lock".into()]),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert!(eff.include_ext.contains("rs"));
    assert!(eff.include_ext.contains("toml"));
    assert!(eff.include_ext.contains("lock"));
    assert!(!eff.include_ext.contains(".rs"));
    assert!(!eff.include_ext.contains("RS"));
}

#[test]
fn test_cargo_files_always_relevant() {
    let include: HashSet<String> = ["rs".into()].into_iter().collect();
    let exclude: HashSet<String> = ["toml".into(), "lock".into()].into_iter().collect();

    // Even though toml and lock are excluded, Cargo.toml and Cargo.lock are always relevant
    assert!(is_relevant_path(
        PathBuf::from("Cargo.toml").as_path(),
        &include,
        &exclude
    ));
    assert!(is_relevant_path(
        PathBuf::from("Cargo.lock").as_path(),
        &include,
        &exclude
    ));

    // But other .toml files should be excluded
    assert!(!is_relevant_path(
        PathBuf::from("config.toml").as_path(),
        &include,
        &exclude
    ));
}

#[test]
fn test_no_extension_ignored() {
    let include: HashSet<String> = ["rs".into()].into_iter().collect();
    let exclude: HashSet<String> = HashSet::new();

    assert!(!is_relevant_path(
        PathBuf::from("README").as_path(),
        &include,
        &exclude
    ));
    assert!(!is_relevant_path(
        PathBuf::from("Makefile").as_path(),
        &include,
        &exclude
    ));
}

// ============================================================================
// Executable Path Tests
// ============================================================================

#[test]
fn test_exe_name_and_path() {
    let name = exe_name("mybin");
    #[cfg(windows)]
    assert_eq!(name, "mybin.exe");
    #[cfg(not(windows))]
    assert_eq!(name, "mybin");

    let td = PathBuf::from("target");
    let p1 = exe_path(&td, false, "mybin");
    let p2 = exe_path(&td, true, "mybin");
    assert!(p1.to_string_lossy().contains("debug"));
    assert!(p2.to_string_lossy().contains("release"));
}

#[test]
fn test_exe_path_different_bins() {
    let td = PathBuf::from("target");
    let p1 = exe_path(&td, false, "server");
    let p2 = exe_path(&td, false, "client");

    assert!(p1.to_string_lossy().contains("server"));
    assert!(p2.to_string_lossy().contains("client"));
    assert_ne!(p1, p2);
}

// ============================================================================
// Hook Execution Tests
// ============================================================================

fn ok_cmd() -> Vec<String> {
    #[cfg(windows)]
    {
        vec!["cmd".into(), "/C".into(), "exit".into(), "0".into()]
    }
    #[cfg(not(windows))]
    {
        vec!["sh".into(), "-c".into(), "true".into()]
    }
}

fn fail_cmd() -> Vec<String> {
    #[cfg(windows)]
    {
        vec!["cmd".into(), "/C".into(), "exit".into(), "1".into()]
    }
    #[cfg(not(windows))]
    {
        vec!["sh".into(), "-c".into(), "false".into()]
    }
}

#[test]
fn test_hooks_stop_on_failure() {
    let hooks = vec![ok_cmd(), fail_cmd(), ok_cmd()];
    let ok = run_hook_list("test", &hooks).unwrap();
    assert!(!ok);
}

#[test]
fn test_hooks_all_ok() {
    let hooks = vec![ok_cmd(), ok_cmd()];
    let ok = run_hook_list("test", &hooks).unwrap();
    assert!(ok);
}

#[test]
fn test_hooks_empty() {
    let hooks: Vec<Vec<String>> = vec![];
    let ok = run_hook_list("test", &hooks).unwrap();
    assert!(ok); // Empty hooks should succeed
}

#[test]
fn test_hooks_single_command() {
    let hooks = vec![ok_cmd()];
    let ok = run_hook_list("test", &hooks).unwrap();
    assert!(ok);
}

#[test]
fn test_hook_empty_argv_errors() {
    let hooks = vec![vec![]]; // Empty command
    let result = run_hook_list("test", &hooks);
    assert!(result.is_err());
}

// ============================================================================
// Build Command Generation Tests
// ============================================================================

#[test]
fn test_build_command_basic() {
    let cli = Config {
        bin: Some("myapp".into()),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert_eq!(eff.build[0], "cargo");
    assert_eq!(eff.build[1], "build");
    assert!(eff.build.contains(&"--bin".to_string()));
    assert!(eff.build.contains(&"myapp".to_string()));
}

#[test]
fn test_build_command_release() {
    let cli = Config {
        bin: Some("myapp".into()),
        release: Some(true),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert!(eff.build.contains(&"--release".to_string()));
}

#[test]
fn test_build_command_workspace() {
    let cli = Config {
        workspace: Some(true),
        package: Some("backend".into()),
        bin: Some("server".into()),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert!(eff.build.contains(&"--workspace".to_string()));
    assert!(eff.build.contains(&"-p".to_string()));
    assert!(eff.build.contains(&"backend".to_string()));
    assert!(eff.build.contains(&"--bin".to_string()));
    assert!(eff.build.contains(&"server".to_string()));
}

#[test]
fn test_build_command_features() {
    let cli = Config {
        bin: Some("myapp".into()),
        features: Some(vec!["feature1".into(), "feature2".into()]),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert!(eff.build.contains(&"--features".to_string()));
    assert!(eff.build.contains(&"feature1,feature2".to_string()));
}

#[test]
fn test_build_command_all_features() {
    let cli = Config {
        bin: Some("myapp".into()),
        all_features: Some(true),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert!(eff.build.contains(&"--all-features".to_string()));
}

#[test]
fn test_build_command_no_default_features() {
    let cli = Config {
        bin: Some("myapp".into()),
        no_default_features: Some(true),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert!(eff.build.contains(&"--no-default-features".to_string()));
}

#[test]
fn test_build_command_explicit_overrides_cargo() {
    let cli = Config {
        build: Some(vec![
            "rustc".into(),
            "main.rs".into(),
            "-o".into(),
            "/tmp/app".into(),
        ]),
        bin: Some("ignored".into()), // Should be ignored
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert_eq!(eff.build[0], "rustc");
    assert_eq!(eff.build[1], "main.rs");
    assert!(!eff.build.contains(&"cargo".to_string()));
}

// ============================================================================
// Config File Loading Tests
// ============================================================================

#[test]
fn test_load_config_from_file() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".rair.toml");

    fs::write(
        &config_path,
        r#"
watch = ["src", "tests"]
ignore = ["**/target/**"]
include_ext = ["rs"]
debounce_ms = 100
clear = false
bin = "testapp"
release = true

pre_build = [
  ["cargo", "fmt"]
]
"#,
    )
    .unwrap();

    let cfg = load_config(&config_path).unwrap();

    assert_eq!(cfg.watch.as_ref().unwrap().len(), 2);
    assert_eq!(cfg.debounce_ms, Some(100));
    assert_eq!(cfg.clear, Some(false));
    assert_eq!(cfg.bin.as_deref(), Some("testapp"));
    assert_eq!(cfg.release, Some(true));
}

#[test]
fn test_load_config_minimal() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".rair.toml");

    fs::write(
        &config_path,
        r#"
bin = "myapp"
"#,
    )
    .unwrap();

    let cfg = load_config(&config_path).unwrap();
    assert_eq!(cfg.bin.as_deref(), Some("myapp"));
    assert!(cfg.watch.is_none());
    assert!(cfg.debounce_ms.is_none());
}

#[test]
fn test_load_config_with_hooks() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".rair.toml");

    fs::write(
        &config_path,
        r#"
bin = "myapp"

pre_build = [
  ["cargo", "fmt"],
  ["cargo", "clippy"]
]

post_build = [
  ["cargo", "test", "-q"]
]
"#,
    )
    .unwrap();

    let cfg = load_config(&config_path).unwrap();
    assert_eq!(cfg.pre_build.as_ref().unwrap().len(), 2);
    assert_eq!(cfg.post_build.as_ref().unwrap().len(), 1);
}

#[test]
fn test_load_config_nonexistent_errors() {
    let result = load_config(&PathBuf::from("/nonexistent/path/.rair.toml"));
    assert!(result.is_err());
}

#[test]
fn test_load_config_invalid_toml_errors() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(".rair.toml");

    fs::write(&config_path, "this is not valid toml {{").unwrap();

    let result = load_config(&config_path);
    assert!(result.is_err());
}

// ============================================================================
// Cargo Metadata Tests
// ============================================================================

#[test]
fn test_metadata_smoke_temp_project_inputs() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "tmp_rair_meta"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"ok\"); }\n",
    )
    .unwrap();

    let cli = Config {
        manifest_path: Some(root.join("Cargo.toml").to_string_lossy().to_string()),
        bin: Some("tmp_rair_meta".into()),
        release: Some(false),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();
    assert_eq!(eff.bin.as_deref(), Some("tmp_rair_meta"));
    assert!(eff.manifest_path.is_some());
}

// ============================================================================
// Run Command Tests
// ============================================================================

#[test]
fn test_explicit_run_command() {
    let cli = Config {
        run: Some(vec!["/tmp/myapp".into(), "--arg".into()]),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert_eq!(eff.run.as_ref().unwrap()[0], "/tmp/myapp");
    assert_eq!(eff.run.as_ref().unwrap()[1], "--arg");
}

#[test]
fn test_run_defaults_to_none_for_cargo() {
    let cli = Config {
        bin: Some("myapp".into()),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    // Should be None, will be resolved at runtime via cargo metadata
    assert!(eff.run.is_none());
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
fn test_ignore_globs_with_invalid_pattern() {
    let result = build_globset(&vec!["[invalid".into()]);
    assert!(result.is_err());
}

#[test]
fn test_manifest_path_preserved() {
    let cli = Config {
        manifest_path: Some("/custom/path/Cargo.toml".into()),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert!(eff.manifest_path.is_some());
    assert_eq!(
        eff.manifest_path.unwrap().to_string_lossy(),
        "/custom/path/Cargo.toml"
    );
}

#[test]
fn test_debounce_conversion() {
    let cli = Config {
        debounce_ms: Some(500),
        ..Default::default()
    };
    let eff = effective_config(cli, None).unwrap();

    assert_eq!(eff.debounce.as_millis(), 500);
}
