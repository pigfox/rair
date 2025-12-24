use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    pub watch: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
    pub include_ext: Option<Vec<String>>,
    pub exclude_ext: Option<Vec<String>>,
    pub debounce_ms: Option<u64>,
    pub clear: Option<bool>,

    /// Optional explicit build argv; if omitted, derived from cargo flags.
    pub build: Option<Vec<String>>,

    /// Optional explicit run argv; if omitted, rair runs the built binary via cargo metadata.
    pub run: Option<Vec<String>>,

    // Cargo-related options
    pub manifest_path: Option<String>,
    pub package: Option<String>,
    pub bin: Option<String>,
    pub features: Option<Vec<String>>,
    pub all_features: Option<bool>,
    pub no_default_features: Option<bool>,
    pub workspace: Option<bool>,
    pub release: Option<bool>,

    // Hooks: list of argv commands (each command is Vec<String>)
    pub pre_build: Option<Vec<Vec<String>>>,
    pub post_build: Option<Vec<Vec<String>>>,
    pub pre_run: Option<Vec<Vec<String>>>,
    pub post_run: Option<Vec<Vec<String>>>,
    pub on_build_fail: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub watch: Vec<PathBuf>,
    pub ignore_globs: Vec<String>,
    pub ignore_set: GlobSet,

    pub include_ext: HashSet<String>,
    pub exclude_ext: HashSet<String>,

    pub debounce: Duration,
    pub clear: bool,

    /// Build argv (always present)
    pub build: Vec<String>,

    /// Optional explicit run argv; if None => run built binary via metadata.
    pub run: Option<Vec<String>>,

    // Cargo selection
    pub manifest_path: Option<PathBuf>,
    pub package: Option<String>,
    pub bin: Option<String>,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub workspace: bool,
    pub release: bool,

    // Hooks
    pub pre_build: Vec<Vec<String>>,
    pub post_build: Vec<Vec<String>>,
    pub pre_run: Vec<Vec<String>>,
    pub post_run: Vec<Vec<String>>,
    pub on_build_fail: Vec<Vec<String>>,
}

pub fn load_config(path: &Path) -> Result<Config> {
    let s = std::fs::read_to_string(path).with_context(|| format!("read config {:?}", path))?;
    let cfg: Config = toml::from_str(&s).with_context(|| format!("parse toml {:?}", path))?;
    Ok(cfg)
}

pub fn build_globset(globs: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for g in globs {
        b.add(Glob::new(g).with_context(|| format!("bad glob: {}", g))?);
    }
    Ok(b.build()?)
}

fn merge_config(mut base: Config, overlay: Config) -> Config {
    if overlay.watch.is_some() {
        base.watch = overlay.watch;
    }
    if overlay.ignore.is_some() {
        base.ignore = overlay.ignore;
    }
    if overlay.include_ext.is_some() {
        base.include_ext = overlay.include_ext;
    }
    if overlay.exclude_ext.is_some() {
        base.exclude_ext = overlay.exclude_ext;
    }
    if overlay.debounce_ms.is_some() {
        base.debounce_ms = overlay.debounce_ms;
    }
    if overlay.clear.is_some() {
        base.clear = overlay.clear;
    }
    if overlay.build.is_some() {
        base.build = overlay.build;
    }
    if overlay.run.is_some() {
        base.run = overlay.run;
    }

    if overlay.manifest_path.is_some() {
        base.manifest_path = overlay.manifest_path;
    }
    if overlay.package.is_some() {
        base.package = overlay.package;
    }
    if overlay.bin.is_some() {
        base.bin = overlay.bin;
    }
    if overlay.features.is_some() {
        base.features = overlay.features;
    }
    if overlay.all_features.is_some() {
        base.all_features = overlay.all_features;
    }
    if overlay.no_default_features.is_some() {
        base.no_default_features = overlay.no_default_features;
    }
    if overlay.workspace.is_some() {
        base.workspace = overlay.workspace;
    }
    if overlay.release.is_some() {
        base.release = overlay.release;
    }

    if overlay.pre_build.is_some() {
        base.pre_build = overlay.pre_build;
    }
    if overlay.post_build.is_some() {
        base.post_build = overlay.post_build;
    }
    if overlay.pre_run.is_some() {
        base.pre_run = overlay.pre_run;
    }
    if overlay.post_run.is_some() {
        base.post_run = overlay.post_run;
    }
    if overlay.on_build_fail.is_some() {
        base.on_build_fail = overlay.on_build_fail;
    }

    base
}

fn norm_ext(s: &str) -> String {
    s.trim().trim_start_matches('.').to_ascii_lowercase()
}

pub fn effective_config(cli: Config, file: Option<Config>) -> Result<EffectiveConfig> {
    let merged = merge_config(file.unwrap_or_default(), cli);

    // Smart default watch paths: if Cargo.toml exists, use Cargo defaults, else use current dir
    let default_watch = if PathBuf::from("Cargo.toml").exists() {
        vec!["src".into(), "Cargo.toml".into(), "Cargo.lock".into()]
    } else {
        vec![".".into()]
    };

    let default_ignore = vec!["**/target/**".into(), "**/.git/**".into()];
    let default_include_ext = vec!["rs".into(), "toml".into()];

    let watch = merged
        .watch
        .unwrap_or(default_watch)
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let ignore_globs = merged.ignore.unwrap_or(default_ignore);
    let ignore_set = build_globset(&ignore_globs)?;

    let include_ext_list = merged.include_ext.unwrap_or(default_include_ext);
    let include_ext: HashSet<String> = include_ext_list.into_iter().map(|e| norm_ext(&e)).collect();

    let exclude_ext: HashSet<String> = merged
        .exclude_ext
        .unwrap_or_default()
        .into_iter()
        .map(|e| norm_ext(&e))
        .collect();

    let debounce_ms = merged.debounce_ms.unwrap_or(250);
    let clear = merged.clear.unwrap_or(true);

    let manifest_path = merged.manifest_path.map(PathBuf::from);
    let package = merged.package;
    let bin = merged.bin;

    let features = merged.features.unwrap_or_default();
    let all_features = merged.all_features.unwrap_or(false);
    let no_default_features = merged.no_default_features.unwrap_or(false);
    let workspace = merged.workspace.unwrap_or(false);
    let release = merged.release.unwrap_or(false);

    let build = merged.build.unwrap_or_else(|| {
        let mut v = vec!["cargo".into(), "build".into()];
        if release {
            v.push("--release".into());
        }
        if let Some(mp) = &manifest_path {
            v.push("--manifest-path".into());
            v.push(mp.to_string_lossy().to_string());
        }
        if workspace {
            v.push("--workspace".into());
        }
        if let Some(p) = &package {
            v.push("-p".into());
            v.push(p.clone());
        }
        if let Some(b) = &bin {
            v.push("--bin".into());
            v.push(b.clone());
        }
        if all_features {
            v.push("--all-features".into());
        }
        if no_default_features {
            v.push("--no-default-features".into());
        }
        if !features.is_empty() {
            v.push("--features".into());
            v.push(features.join(","));
        }
        v
    });

    let pre_build = merged.pre_build.unwrap_or_default();
    let post_build = merged.post_build.unwrap_or_default();
    let pre_run = merged.pre_run.unwrap_or_default();
    let post_run = merged.post_run.unwrap_or_default();
    let on_build_fail = merged.on_build_fail.unwrap_or_default();

    Ok(EffectiveConfig {
        watch,
        ignore_globs,
        ignore_set,
        include_ext,
        exclude_ext,
        debounce: Duration::from_millis(debounce_ms),
        clear,
        build,
        run: merged.run,
        manifest_path,
        package,
        bin,
        features,
        all_features,
        no_default_features,
        workspace,
        release,
        pre_build,
        post_build,
        pre_run,
        post_run,
        on_build_fail,
    })
}

/// Returns true if this path should trigger rebuild/restart.
pub fn is_relevant_path(
    path: &Path,
    include_ext: &HashSet<String>,
    exclude_ext: &HashSet<String>,
) -> bool {
    // Always treat Cargo manifest/lock as relevant.
    if path.ends_with("Cargo.toml") || path.ends_with("Cargo.lock") {
        return true;
    }

    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext.is_empty() {
        return false;
    }
    if exclude_ext.contains(&ext) {
        return false;
    }
    include_ext.contains(&ext)
}

pub fn exe_name(bin: &str) -> String {
    #[cfg(windows)]
    {
        format!("{}.exe", bin)
    }

    #[cfg(not(windows))]
    {
        bin.to_string()
    }
}

pub fn exe_path(target_dir: &Path, release: bool, bin: &str) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    target_dir.join(profile).join(exe_name(bin))
}

/// Runs a list of hook commands, each an argv vector.
/// Returns Ok(true) if all commands succeed, Ok(false) if any fails.
pub fn run_hook_list(name: &str, hooks: &[Vec<String>]) -> Result<bool> {
    if hooks.is_empty() {
        return Ok(true);
    }
    for (i, argv) in hooks.iter().enumerate() {
        anyhow::ensure!(!argv.is_empty(), "hook {}[{}] argv is empty", name, i);
        let mut c = Command::new(&argv[0]);
        if argv.len() > 1 {
            c.args(&argv[1..]);
        }
        let status = c
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("hook {}[{}]: {:?}", name, i, argv))?;
        if !status.success() {
            return Ok(false);
        }
    }
    Ok(true)
}
