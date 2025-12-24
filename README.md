# rair

Air-like hot reload for Rust (Windows/macOS/Linux). Works with both Cargo projects and standalone `.rs` files.

## Features

- **Dual mode**: Cargo projects AND standalone `.rs` files
- **Simple interface**: `rair main.rs` or `rair --bin my_app`
- **Cross-platform**: Windows/macOS/Linux (no bash required)
- **Smart defaults**: Auto-detects Cargo vs standalone environments
- Watch files/dirs with debounce
- Ignore globs (`**/target/**`, `**/.git/**`, etc.)
- Clear screen on restart with proper cursor positioning
- Timestamped logs
- Build step + restart only on successful build
- Kills the entire process tree on restart (process groups / job objects)
- **Cargo integration**:
  - `--manifest-path`
  - `-p/--package`, `--bin`, `--workspace`
  - `--release`, `--features`, `--all-features`, `--no-default-features`
- Runs the built binary directly using `cargo metadata` (avoids extra work from `cargo run`)
- **Air-style hooks**:
  - `pre_build`, `post_build`, `pre_run`, `post_run`, `on_build_fail`

## Install
```bash
cargo install --path .
```

Verify:
```bash
rair --help
```
## Testing

### Run the test suite
```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_config_merge_cli_wins
```

### Development workflow with tests

Use hooks in `.rair.toml` to run tests automatically:
```toml
bin = "my_app"

pre_build = [
  ["cargo", "fmt"]
]

post_build = [
  ["cargo", "test", "-q"]  # Runs tests after successful build
]
```

Now `rair` will run your tests after every successful build.

### Manual testing without auto-run

If you don't want tests running automatically:
```bash
# Terminal 1: Run rair
rair --bin my_app

# Terminal 2: Run tests manually when needed
cargo test
```

Or use different configs for development vs testing:
```bash
# Development (no tests)
rair --bin my_app

# With tests
rair --config .rair-test.toml
```

Where `.rair-test.toml` includes the `post_build` hook.

## Quick Start

### Standalone .rs files (learning Rust, quick scripts)
```bash
# Watch and compile a single file
rair main.rs

# Watch a specific file in any directory
cd rust-by-example/3.CustomTypes
rair enums.rs
```

### Cargo projects
```bash
# Standard Cargo project
rair --bin my_app

# Cargo workspace
rair --workspace -p my_crate --bin my_bin

# Release mode
rair --bin my_app --release
```

### Using config files

For repeated use or complex setups, create `.rair.toml`:
```bash
# Copy an example config
cp examples/cargo-project.rair.toml .rair.toml

# Or create your own (see examples/ directory)
# Then just run:
rair
```

## Usage Examples

### Example 1: Learning Rust with standalone files
```bash
cd ~/learning-rust
rair variables.rs
# Edit variables.rs → auto-recompiles and reruns
```

### Example 2: Standard Cargo project
```bash
cd my-rust-app
rair --bin my_app
# Edit src/main.rs → auto-rebuilds and restarts
```

### Example 3: Cargo workspace with tests

Create `.rair.toml`:
```toml
workspace = true
package = "backend"
bin = "server"

pre_build = [["cargo", "fmt"]]
post_build = [["cargo", "test", "-q"]]
```

Run:
```bash
rair
```

### Example 4: Multiple .rs files (auto-compile latest)

For directories with multiple standalone `.rs` files:
```bash
cp examples/standalone-auto.rair.toml .rair.toml
rair
# Edit any .rs file → compiles the most recently modified one
```

## Configuration

### Config file (`.rair.toml`)

See the `examples/` directory for ready-to-use configs:
- `cargo-project.rair.toml` - Standard Cargo with hooks
- `cargo-workspace.rair.toml` - Workspace setup
- `simple-cargo.rair.toml` - Minimal Cargo config
- `standalone-auto.rair.toml` - Auto-detect latest .rs file

Full config example:
```toml
watch = ["src", "Cargo.toml", "Cargo.lock"]
ignore = ["**/target/**", "**/.git/**"]
include_ext = ["rs", "toml"]
debounce_ms = 250
clear = true

bin = "my_app"

pre_build = [
  ["cargo", "fmt"]
]

post_build = [
  ["cargo", "test", "-q"]
]
```

### CLI Options
```bash
rair [FILES]... [OPTIONS]

Arguments:
  [FILES]...              Rust files to watch (e.g., rair main.rs)

Options:
  --config <FILE>         Config file path (default: .rair.toml)
  --watch <PATH>...       Watch paths (repeatable)
  --ignore <GLOB>...      Ignore globs (repeatable)
  --include-ext <EXT>...  Include extensions (default: rs,toml)
  --debounce-ms <MS>      Debounce in ms (default: 250)
  --clear                 Clear screen before run
  --build <CMD>...        Explicit build command
  --run <CMD>...          Explicit run command
  --bin <NAME>            Binary name (Cargo projects)
  -p, --package <NAME>    Package name (workspaces)
  --workspace             Build workspace
  --release               Release mode
  --features <LIST>...    Enable features
  --all-features          Enable all features
```

## How It Works

### Auto-detection

`rair` automatically detects your environment:

- **`Cargo.toml` exists?** → Cargo mode (watches `src/`, `Cargo.toml`, `Cargo.lock`)
- **No `Cargo.toml`?** → Standalone mode (watches current directory)
- **Files provided as args?** → Direct file mode (compiles specified files)

### Priority

Settings are merged in this order (later overrides earlier):

1. Built-in defaults
2. `.rair.toml` file (if present)
3. CLI arguments
4. File arguments (highest priority)

## Notes

- Build failures keep the current process running
- In workspaces, always specify `--bin`
- Hooks are optional and only run if configured
- File mode (`rair main.rs`) ignores config files for simplicity

## Why rair?

- **Simpler than Air**: No Go dependency, pure Rust
- **Dual mode**: Works with Cargo projects AND standalone files
- **Smart defaults**: Automatically adapts to your project structure
- **Proper process management**: Kills entire process tree on restart
- **Cross-platform**: Windows, macOS, Linux support

## License

MIT