# Windows development tasks use the built-in PowerShell shell.
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# List the available development commands.
default:
    @just --list

# Install or refresh the locked frontend development dependencies.
setup:
    npm.cmd --prefix apps/orally-desktop ci --include=dev --prefer-offline --no-audit --no-fund

# Start the desktop app with Vite hot reload and Rust rebuilds.
[working-directory("apps/orally-desktop")]
dev: _ensure-deps
    npm.cmd run dev

# Start only the frontend at http://localhost:1420.
[working-directory("apps/orally-desktop")]
dev-ui: _ensure-deps
    npm.cmd run ui:dev

# Build the release desktop app and prepare dist/portable/Orally.
build:
    powershell.exe -NoLogo -NoProfile -File scripts/package-portable.ps1

# Build all Rust workspace packages in debug mode.
build-debug:
    cargo build --workspace

# Run the Rust workspace tests.
test:
    cargo test --workspace

# Check Rust formatting and compilation without building executables.
check:
    cargo fmt --all -- --check
    cargo check --workspace

# Format the Rust workspace.
fmt:
    cargo fmt --all

# Reuse installed dependencies for fast startup; use setup after lockfile changes.
[private]
[working-directory("apps/orally-desktop")]
_ensure-deps:
    @if (!(Test-Path 'node_modules/.bin/tauri.cmd') -or !(Test-Path 'node_modules/.bin/vite.cmd')) { npm.cmd ci --include=dev --prefer-offline --no-audit --no-fund; exit $LASTEXITCODE }
