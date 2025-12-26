use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use chrono::Local;
use command_group::{CommandGroup, GroupChild};
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    time::Instant,
};

use clap::Parser;
use rair::{Config, EffectiveConfig};

#[derive(Parser, Debug, Clone)]
#[command(name = "rair", about = "Air-like hot reload for Rust (cross-platform)")]
struct Cli {
    /// Rust files to watch and compile (e.g., rair main.rs, rair *.rs)
    files: Vec<PathBuf>,

    /// Config file path (default: .rair.toml if present)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Watch paths (repeatable)
    #[arg(long)]
    watch: Vec<String>,

    /// Ignore globs (repeatable)
    #[arg(long)]
    ignore: Vec<String>,

    /// Include file extensions (repeatable). Default: rs,toml
    #[arg(long)]
    include_ext: Vec<String>,

    /// Exclude file extensions (repeatable)
    #[arg(long)]
    exclude_ext: Vec<String>,

    /// Debounce in ms
    #[arg(long)]
    debounce_ms: Option<u64>,

    /// Clear screen before run
    #[arg(long)]
    clear: Option<bool>,

    /// Explicit build command argv (single command)
    #[arg(long, num_args = 1.., allow_hyphen_values = true)]
    build: Vec<String>,

    /// Explicit run command argv (single command)
    #[arg(long, num_args = 1.., allow_hyphen_values = true)]
    run: Vec<String>,

    /// Cargo.toml path
    #[arg(long)]
    manifest_path: Option<String>,

    /// Package name (workspace)
    #[arg(short = 'p', long)]
    package: Option<String>,

    /// Binary name to run
    #[arg(long)]
    bin: Option<String>,

    /// Cargo features (repeatable)
    #[arg(long)]
    features: Vec<String>,

    #[arg(long)]
    all_features: bool,

    #[arg(long)]
    no_default_features: bool,

    #[arg(long)]
    workspace: bool,

    #[arg(long)]
    release: bool,
}

fn ts() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn log_info(msg: &str) {
    eprintln!("[{}] {}", ts(), msg);
}

fn clear_screen() -> Result<()> {
    execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}

fn cmd_from_argv(argv: &[String]) -> Result<Command> {
    anyhow::ensure!(!argv.is_empty(), "command argv cannot be empty");
    let mut c = Command::new(&argv[0]);
    if argv.len() > 1 {
        c.args(&argv[1..]);
    }
    Ok(c)
}

fn run_build(build: &[String]) -> Result<bool> {
    log_info(&format!("build: {:?}", build));
    let mut c = cmd_from_argv(build)?;
    let status = c
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("build: {:?}", build))?;
    Ok(status.success())
}

fn spawn_run_group(run: &[String]) -> Result<GroupChild> {
    log_info(&format!("run: {:?}", run));
    let mut c = cmd_from_argv(run)?;

    // Set environment variable to prevent recursive watching
    c.env("RAIR_ACTIVE", "1");

    let child = c
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .group_spawn()
        .with_context(|| format!("run: {:?}", run))?;
    Ok(child)
}

fn kill_group(child: &mut GroupChild) {
    let _ = child.kill();
    let _ = child.wait();
}

fn load_cfg_file(path: Option<PathBuf>) -> Option<Config> {
    let p = match path {
        Some(p) => p,
        None => {
            let d = PathBuf::from(".rair.toml");
            if d.exists() {
                d
            } else {
                return None;
            }
        }
    };

    match rair::load_config(&p) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            eprintln!("[{}] rair: failed to load {:?}: {:#}", ts(), p, e);
            None
        }
    }
}

fn files_mode_config(files: Vec<PathBuf>) -> Result<Config> {
    anyhow::ensure!(!files.is_empty(), "no files provided");

    // Verify all files exist and are .rs files
    for f in &files {
        anyhow::ensure!(f.exists(), "file does not exist: {:?}", f);
        anyhow::ensure!(
            f.extension().and_then(|s| s.to_str()) == Some("rs"),
            "not a .rs file: {:?}",
            f
        );
    }

    // Build command: compile all files into /tmp/rair-out
    let mut build_cmd = vec!["rustc".to_string()];
    for f in &files {
        build_cmd.push(f.to_string_lossy().to_string());
    }
    build_cmd.push("-o".to_string());
    build_cmd.push("/tmp/rair-out".to_string());

    Ok(Config {
        watch: Some(vec![".".to_string()]), // Always watch current directory
        include_ext: Some(vec!["rs".to_string()]),
        ignore: Some(vec!["**/target/**".to_string(), "**/.git/**".to_string()]),
        build: Some(build_cmd),
        run: Some(vec!["/tmp/rair-out".to_string()]),
        clear: Some(true),
        ..Default::default()
    })
}

fn cli_to_config(cli: Cli) -> Result<Config> {
    // If files are provided, use files mode
    if !cli.files.is_empty() {
        return files_mode_config(cli.files);
    }

    // Otherwise use flag-based mode
    Ok(Config {
        watch: if cli.watch.is_empty() {
            None
        } else {
            Some(cli.watch)
        },
        ignore: if cli.ignore.is_empty() {
            None
        } else {
            Some(cli.ignore)
        },
        include_ext: if cli.include_ext.is_empty() {
            None
        } else {
            Some(cli.include_ext)
        },
        exclude_ext: if cli.exclude_ext.is_empty() {
            None
        } else {
            Some(cli.exclude_ext)
        },
        debounce_ms: cli.debounce_ms,
        clear: cli.clear,
        build: if cli.build.is_empty() {
            None
        } else {
            Some(cli.build)
        },
        run: if cli.run.is_empty() {
            None
        } else {
            Some(cli.run)
        },

        manifest_path: cli.manifest_path,
        package: cli.package,
        bin: cli.bin,
        features: if cli.features.is_empty() {
            None
        } else {
            Some(cli.features)
        },
        all_features: Some(cli.all_features),
        no_default_features: Some(cli.no_default_features),
        workspace: Some(cli.workspace),
        release: Some(cli.release),

        pre_build: None,
        post_build: None,
        pre_run: None,
        post_run: None,
        on_build_fail: None,
    })
}

fn cargo_metadata_target_dir(manifest_path: Option<&PathBuf>) -> Result<PathBuf> {
    let mut cmd = MetadataCommand::new();
    if let Some(mp) = manifest_path {
        cmd.manifest_path(mp);
    }
    let md = cmd.exec().context("cargo metadata")?;
    Ok(md.target_directory.into_std_path_buf())
}

fn resolve_bin_name(eff: &EffectiveConfig) -> Result<String> {
    if let Some(b) = &eff.bin {
        return Ok(b.clone());
    }
    if let Some(p) = &eff.package {
        return Ok(p.clone());
    }
    let cwd = std::env::current_dir().context("cwd")?;
    let name = cwd
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("cannot infer bin name; specify --bin or config bin"))?;
    Ok(name.to_string())
}

fn build_default_run_argv(eff: &EffectiveConfig) -> Result<Vec<String>> {
    let target_dir = cargo_metadata_target_dir(eff.manifest_path.as_ref())?;
    let bin = resolve_bin_name(eff)?;
    let exe = rair::exe_path(&target_dir, eff.release, &bin);
    Ok(vec![exe.to_string_lossy().to_string()])
}

fn run_post_run_hooks(eff: &EffectiveConfig) {
    match rair::run_hook_list("post_run", &eff.post_run) {
        Ok(true) => {}
        Ok(false) => log_info("post_run hook failed (ignored)"),
        Err(e) => log_info(&format!("post_run hook error (ignored): {:#}", e)),
    }
}

fn main() -> Result<()> {
    // Prevent recursive watching - if we're already being watched by rair, don't watch again
    if std::env::var("RAIR_ACTIVE").is_ok() {
        eprintln!("Error: rair is already watching this process");
        eprintln!("Hint: You cannot use rair to watch itself (prevents infinite recursion)");
        eprintln!("To develop rair, use: cargo watch -x test");
        std::process::exit(1);
    }

    let cli = Cli::parse();

    // Determine config source priority:
    // 1. If files provided as args → use files mode (ignore config file)
    // 2. Otherwise → merge config file + CLI flags
    let (cli_cfg, file_cfg) = if !cli.files.is_empty() {
        (cli_to_config(cli)?, None)
    } else {
        (
            cli_to_config(cli.clone())?,
            load_cfg_file(cli.config.clone()),
        )
    };

    let eff: EffectiveConfig = rair::effective_config(cli_cfg, file_cfg)?;

    let child: Arc<Mutex<Option<GroupChild>>> = Arc::new(Mutex::new(None));

    // watcher channel
    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher =
        RecommendedWatcher::new(tx, notify::Config::default()).context("create watcher")?;

    let mut watched_any = false;
    for p in &eff.watch {
        if !p.exists() {
            log_info(&format!("watch path missing (skipped): {:?}", p));
            continue;
        }
        watcher
            .watch(p, RecursiveMode::Recursive)
            .with_context(|| format!("watch {:?}", p))?;
        watched_any = true;
    }
    anyhow::ensure!(watched_any, "no watch paths exist");

    // Start / restart helper
    let start_app = |eff: &EffectiveConfig, child: &Arc<Mutex<Option<GroupChild>>>| -> Result<()> {
        // pre_build
        if !rair::run_hook_list("pre_build", &eff.pre_build)? {
            log_info("pre_build failed; skipping build");
            return Ok(());
        }

        // build
        let ok = run_build(&eff.build)?;
        if !ok {
            let _ = rair::run_hook_list("on_build_fail", &eff.on_build_fail);
            log_info("build failed; keeping existing process");
            return Ok(());
        }

        // post_build
        if !rair::run_hook_list("post_build", &eff.post_build)? {
            log_info("post_build failed; keeping existing process");
            return Ok(());
        }

        // pre_run
        if !rair::run_hook_list("pre_run", &eff.pre_run)? {
            log_info("pre_run failed; keeping existing process");
            return Ok(());
        }

        // determine run argv
        let run_argv = match &eff.run {
            Some(v) => v.clone(),
            None => build_default_run_argv(eff)?,
        };

        // restart
        {
            let mut guard = child.lock().unwrap();
            if let Some(ch) = guard.as_mut() {
                log_info("stopping previous process");
                kill_group(ch);
            }
            if eff.clear {
                clear_screen()?;
            }
            *guard = Some(spawn_run_group(&run_argv)?);
        }

        run_post_run_hooks(eff);
        Ok(())
    };

    // initial start
    start_app(&eff, &child)?;

    // debounce loop
    let mut last = Instant::now() - eff.debounce;
    loop {
        let evt = rx.recv().context("watch recv")?;
        let now = Instant::now();
        if now.duration_since(last) < eff.debounce {
            continue;
        }
        last = now;

        let event = match evt {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[{}] watch error: {:#}", ts(), e);
                continue;
            }
        };

        // ignore + relevance filter
        let mut relevant = false;
        for p in &event.paths {
            if eff.ignore_set.is_match(p) {
                continue;
            }
            if rair::is_relevant_path(p, &eff.include_ext, &eff.exclude_ext) {
                relevant = true;
                break;
            }
        }
        if !relevant {
            continue;
        }

        // rebuild + restart policy
        start_app(&eff, &child)?;

        io::stdout().flush().ok();
    }
}
