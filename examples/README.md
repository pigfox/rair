# rair Configuration Examples

## Cargo Project (Standard)

Copy to your Cargo project root:
```bash
cp examples/cargo-project.rair.toml .rair.toml
# Edit bin = "my_app" to match your binary name
rair
```

Or just use CLI:
```bash
rair --bin my_app
```

## Cargo Workspace

Copy to your workspace root:
```bash
cp examples/cargo-workspace.rair.toml .rair.toml
# Edit package and bin names
rair
```

Or use CLI:
```bash
rair --workspace -p my_crate --bin my_bin
```

## Standalone .rs Files (Auto-compile latest)

For directories with multiple .rs files, auto-compiles the most recently modified:
```bash
cp examples/standalone-auto.rair.toml .rair.toml
rair
```

Or use CLI for a specific file:
```bash
rair enums.rs
```

## Simple Cargo (Minimal Config)

For basic projects, minimal config:
```bash
cp examples/simple-cargo.rair.toml .rair.toml
rair
```
