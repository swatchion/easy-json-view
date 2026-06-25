**English** · [简体中文](./README.zh-CN.md)

# Easy Json View

`OSX user need to run sudo xattr -d com.apple.quarantine /Applications/EasyJsonView.app`

A modern JSON formatter / validator built with **Rust + Dioxus 0.7.9**, targeting **both desktop and Web from a single codebase**:

- **Desktop** (default): a native window app (wry/webkit2gtk) — no browser, no server; history and config are stored in a single `store.json` under the user's config directory.
- **Web**: compiled to **WebAssembly** and run in the browser; history and config are stored in the browser's `localStorage`.

The core JSON logic and all features are shared across both targets; only "storage" and a few "DOM/system" helpers fork by platform.

## ✨ Features

- 🚀 **High performance**: built on Rust + WebAssembly for fast processing; output is rendered as a single text block with line numbers, staying smooth even for large documents
- 🎨 **Formatting (beautify)**: choose 2 / 4 / 8 space indentation
- 🌈 **Syntax highlighting**: colorizes small-to-medium output (keys / strings / numbers / booleans / null); very large documents automatically fall back to plain text to preserve performance
- 🗜️ **Minify**: strips whitespace and compresses to a single line
- 🔤 **Key order control**: preserves the original object key order by default; optionally "sort keys" alphabetically
- 🔎 **Key/value search in results**: find keys/values within the formatted output, highlight all matches with a count, jump to previous / next (Enter / Shift+Enter), and toggle case sensitivity; match counts are capped for very large documents to preserve performance
- 📊 **JSON statistics**: counts objects / arrays / keys / strings / numbers / booleans / null and the total number of values
- 💾 **History** (localStorage): auto-save, search, rename, delete, clear; deduplicated by content, capped at 100 entries
- ⭐ **Bookmarks**: pin important records to the top so they are never evicted, with a "bookmarks only" filter; re-formatting the same content keeps its bookmark
- 📋 **Copy / Download / Import**: one-click copy of the output or input, download output as `.json`, import JSON from a local file
- ⚠️ **Error reporting**: an inline error banner with serde's line / column location
- ⌨️ **Shortcuts**: `Ctrl+Enter` (or `Cmd+Enter`) to format quickly
- 🧩 **Sample / Clear**: load a built-in sample or clear the input and output with one click
- ⚙️ **Persistent settings**: indentation size and sort-keys option are saved across reloads
- 📱 **Responsive & accessible**: stacks automatically on mobile, with `aria-label`s and focus rings

## 🛠️ Tech Stack

- **UI framework**: Dioxus 0.7.9 (`desktop` / `web` dual renderer, one per target)
- **Language**: Rust (edition 2021)
- **Compile targets**: native (desktop) / WebAssembly (`wasm32-unknown-unknown`, Web)
- **Data storage**: a single `~/.config/easy-json-view/store.json` on desktop / browser `localStorage` on Web (routed per platform by the `src/platform/` shim)
- **Styling**: Tailwind CSS (**offline**; the standalone CLI generates `assets/tailwind.css`, loaded via `asset!` + `document::Stylesheet` — no CDN)
- **Build tool**: `dx` (dioxus-cli)
- **JSON processing**: `serde_json` (with `preserve_order` enabled to keep key order)

## 🚀 Getting Started

### Prerequisites

```bash
# dx CLI (one-time); the version must match dioxus in Cargo.toml
cargo install dioxus-cli --version 0.7.9 --locked

# Desktop system dependencies (wry/webkit2gtk)
sudo dnf install webkit2gtk4.1-devel libsoup3-devel     # Fedora
# sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev # Debian/Ubuntu

# Web target
rustup target add wasm32-unknown-unknown
```

> The Tailwind standalone CLI is downloaded automatically by the build scripts (`./tailwindcss`, already in `.gitignore`).

### Desktop (native window)

```bash
./build-desktop.sh            # generate styles → logic tests → dx serve (opens a native window)
./build-desktop.sh build      # build the executable only
# equivalent manual command: dx serve --platform desktop
```

### Web

```bash
./build-web.sh                # generate styles → dx build --platform web → logic tests
./build-web.sh serve          # local preview (dx serve --platform web)
```

The static Web artifacts are placed in `target/dx/easy-json-view/release/web/public/` and can be hosted by any static server.

> Note: at the end of a release build, the `wasm-opt` optimization step may crash due to debug info (DWARF) and print `ERROR ... wasm-opt failed`; dx then falls back to the unoptimized wasm — the build still succeeds and runs fine, only the size is larger (a known dx 0.7 behavior, not a build failure).

> Note: `cargo build`/`test`/`bench` default to the desktop feature (which needs webkit); for pure-logic work add `--no-default-features`.

## 📁 Project Structure

```
├── LICENSE                         # PolyForm Noncommercial 1.0.0 (exemption preamble + verbatim text)
├── README.md                       # main docs (English, default)
├── README.zh-CN.md                 # main docs (Simplified Chinese)
├── index.html                      # Web page skeleton (anti-flicker script + mount point only; CDN and loading screen removed)
├── Dioxus.toml                     # dx config ([web.app] title, etc.)
├── tailwind.config.js              # Tailwind content scans src/**/*.rs + index.html
├── build-desktop.sh / build-web.sh # desktop / Web build scripts (replacing the old build.sh + serve.py)
├── Cargo.toml                      # dual-target dependencies and [features] (default=desktop / web / desktop)
├── assets/
│   ├── input.css                   # Tailwind input (@tailwind + global/dark overrides)
│   └── tailwind.css                # generated offline styles (loaded via asset!)
├── src/
│   ├── main.rs                     # entry point (cfg branch: wasm launch / desktop LaunchBuilder + window)
│   ├── lib.rs                      # library entry; exposes services to benches/tests; mounts platform
│   ├── app.rs                      # all UI (cfg-forked platform helpers)
│   ├── platform/                   # seam 1: Storage platform shim
│   │   ├── mod.rs                  #   routes by cfg
│   │   ├── web.rs                  #   localStorage (gloo)
│   │   └── desktop.rs              #   single store.json file
│   ├── services/
│   │   └── mod_enhanced.rs         # all business logic and types (platform-agnostic)
│   └── tests.rs                    # unit tests
└── benches/
    └── json_performance.rs         # Criterion benchmarks
```

- `app.rs`: the entire UI and event handling, with Tailwind classes written directly in RSX; platform-specific helpers (clipboard/download/timing/file import) fork by `cfg` while keeping their signatures.
- `platform/`: the isomorphic `Storage::{get,set,delete}` shim — Web → localStorage, desktop → `store.json`.
- `services/mod_enhanced.rs` (platform-agnostic):
  - `JsonService` — `validate` / `format` / `minify` / `get_stats` (based on `serde_json`)
  - `HistoryService` — history CRUD (key=`easy_json_view_history`, deduplicated, capped at 100)
  - `ConfigService` — config read/write (key=`easy_json_view_config`)
  - Types: `HistoryRecord`, `FormatOptions`, `ValidationResult`, `JsonStats`, `AppConfig`, `UiSettings`, `TreeRow`

## 💾 Data Storage

History and config are persisted per platform via the `src/platform/` shim — no server, nothing uploaded remotely:

- **Desktop**: a single JSON file `store.json` under your OS config directory (location varies via `dirs::config_dir()`), containing the two keys `easy_json_view_history` and `easy_json_view_config`:
  - **Linux**: `~/.config/easy-json-view/store.json`
  - **macOS**: `~/Library/Application Support/easy-json-view/store.json`
  - **Windows**: `%APPDATA%\easy-json-view\store.json` (Roaming)
- **Web**: browser `localStorage`, with the same two keys.
- `easy_json_view_history` — the history list (deduplicated by content; the default name is the first 7 chars of the content's SHA1 short hash, with a creation timestamp; bookmarks are never evicted). Normal saves are capped at 100 records; **importing a `.zip` can exceed this** — the imported batch is kept in full, and that larger size becomes the new high-water mark for subsequent FIFO rotation.
- `easy_json_view_config` — formatting options (indentation + sort keys) and UI settings (theme / font size / auto-format / language / density / line numbers).

**Sync across devices.** Since desktop history is a single plain `store.json`, you can version/sync it with git — run `git init` in that config directory (or include `store.json` in an already-synced repo). Alternatively, use the desktop **Export / Import (.zip)** buttons in the history sidebar to move history between devices (import merges into the current session, deduplicated by content).

## 🧪 Testing & Benchmarks

```bash
# Pure-logic unit tests (no webkit needed, recommended)
cargo test --lib --no-default-features

# A single test (matched by name)
cargo test test_json_formatting --no-default-features

# Benchmarks (Criterion)
cargo bench --no-default-features
```

## ⚠️ Known limitations (Linux)

- **Window control buttons (Wayland)**: under a Wayland session the window's minimize / maximize / close buttons follow the GTK default (right side) and do **not** follow a GNOME global "macOS-style top-left" layout — a known limitation of tao's bare GTK window on Wayland. If you prefer the left-side layout (or otherwise want the buttons to honor your GNOME setting), launch via XWayland: `GDK_BACKEND=x11 ./easy-json-view` (trade-off: XWayland may look slightly blurry on HiDPI displays).

## 🗺️ Roadmap

- JSON string escape / unescape
- A fully flicker-free dark startup: light-mode startup is now flush (the giant unstyled-icon flash is fixed and the first frame uses the app's light background); dark-mode users may still see a brief light first frame, since the native first-frame background color cannot be theme-aware

## 📄 License

This project is licensed under the **[PolyForm Noncommercial License 1.0.0](./LICENSE)** (SPDX: `PolyForm-Noncommercial-1.0.0`).

- **Source-available, noncommercial**: the source code is publicly readable and may be used for any **noncommercial purpose** — personal study, research, experimentation, use by nonprofit organizations, and so on; **any form of commercial use requires separate authorization**. This is a source-available noncommercial license and is **not an "open source" license** as defined by the OSI.
- **The original developer is not bound by this restriction**: the noncommercial restriction applies only to licensees (the "you" in the license); the copyright holder (the licensor) retains all rights in the software — including use, modification, distribution, sublicensing, and commercial exploitation — and may grant commercial licenses to others. See the exemption NOTICE at the top of `LICENSE`.

See the full terms in [`LICENSE`](./LICENSE) at the repository root.
