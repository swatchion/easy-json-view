# EazyJsonView 构建说明（Windows / macOS / Linux 桌面 + Web）

本文档说明如何分别构建 **Windows、macOS、Linux 三种桌面安装包** 与 **Web（WASM）静态站点**。

> 工具链：**Rust + Dioxus 0.7.9 + dx (dioxus-cli) 0.7.9**。
> 项目为「桌面 + Web 双目标」：核心逻辑两端共用，仅渲染器（`[features]` 的 `desktop` / `web`）与少量平台辅助按目标分叉。

---

## 0. 最重要的前提：桌面版必须在「目标操作系统」上构建

桌面版基于 **wry**（系统 WebView）：

| 平台 | WebView 后端 | 能否跨平台交叉编译 |
|------|--------------|--------------------|
| Windows | WebView2（Edge）| ❌ 实际不可行 |
| macOS | WKWebView（系统自带）| ❌ 实际不可行 |
| Linux | WebKitGTK（webkit2gtk-4.1）| ❌ 实际不可行 |

GUI 桌面应用**不能简单地从一个系统交叉编译到另一个系统**（链接系统 WebView/原生库）。要拿到三平台安装包，二选一：

1. **在三台对应机器/虚拟机上分别构建**（本机或 VM）。
2. **用 CI 矩阵**（GitHub Actions：`ubuntu-latest` / `macos-latest` / `windows-latest` 各跑一份）——见 [§6 CI 矩阵](#6-用-ci-一次产出全部平台github-actions-推荐)。这是产出全部平台最省心的方式。

**Web 版例外**：Web 编译到 `wasm32-unknown-unknown`，与宿主系统无关，在任意一台机器上即可构建。

---

## 1. 通用前置（所有目标都需要）

### 1.1 Rust 工具链

通过 [rustup](https://rustup.rs) 安装 stable Rust（含 `cargo`）。

```bash
# Web 目标需要 WASM 编译目标（桌面不需要）
rustup target add wasm32-unknown-unknown
```

### 1.2 dx (dioxus-cli) —— 版本须与依赖的 dioxus 一致（0.7.9）

```bash
cargo install dioxus-cli --version 0.7.9 --locked
dx --version    # 应输出 dioxus 0.7.9
```

### 1.3 Tailwind 离线样式（**每次构建前必须存在 `assets/tailwind.css`**）

样式经 `app.rs` 的 `asset!("/assets/tailwind.css")` 在**编译期**引入，因此该文件必须先生成。
项目用 Tailwind v3 **standalone CLI**（无需 Node）由 `assets/input.css` + `tailwind.config.js` 生成：

```bash
# 命令本身跨平台一致，区别只在 CLI 可执行文件：
./tailwindcss -c tailwind.config.js -i assets/input.css -o assets/tailwind.css --minify
```

按构建机的操作系统下载对应的 standalone CLI（版本 **v3.4.17**，与仓库脚本一致）：

| 构建机 | 下载文件名（来自 tailwindlabs/tailwindcss releases v3.4.17）|
|--------|-----------------------------------------------------------|
| Linux x64 | `tailwindcss-linux-x64` |
| macOS Apple Silicon | `tailwindcss-macos-arm64` |
| macOS Intel | `tailwindcss-macos-x64` |
| Windows x64 | `tailwindcss-windows-x64.exe` |

> 仓库内 `build-desktop.sh` / `build-web.sh` 会自动下载 **linux-x64** 版——故这两个脚本仅适用于 Linux 构建机。在 macOS/Windows 上请手动下载对应 CLI 后执行上面的生成命令（或改脚本里的下载 URL）。
>
> `assets/tailwind.css` 已纳入版本库；**只有当你改了 RSX 里的类名或 `input.css`/`tailwind.config.js` 后**才必须重新生成。

---

## 2. Web 版（WASM，跨系统通用）

```bash
# 0) 确保已生成 assets/tailwind.css（见 §1.3）
# 1) 构建发布产物
dx build --platform web --release
```

或用脚本（仅 Linux 构建机）：`./build-web.sh`（构建）、`./build-web.sh serve`（本地预览）。

**产物目录**（静态文件，可托管到任意静态服务器 / CDN / 对象存储）：

```
target/dx/eazy-json-view/release/web/public/
```

本地预览：

```bash
dx serve --platform web        # 默认 http://127.0.0.1:8080
```

**注意事项**

- ⚠️ **`wasm-opt` 体积优化崩溃属已知现象、非构建失败**：`dx build --platform web --release` 末尾的 `wasm-opt` 步骤会因调试信息（DWARF）报 `ERROR ... wasm-opt failed`，dx 随即**回退到未优化的 wasm**——构建仍标记成功、产物可正常运行，只是体积偏大。**勿误判为失败。**
- 若要部署在**子路径**（如 `https://example.com/json/`）而非站点根，请在 `Dioxus.toml` 的 `[web.app]` 增加 `base_path = "json"` 后重新构建。
- Web 端历史/配置存浏览器 `localStorage`；首屏防深色闪烁脚本在 `index.html`。
- JetBrains Mono 字体在 Web 端经 `index.html` 的 Google Fonts `<link>` 在线加载（离线环境回退系统等宽字体）。

---

## 3. 桌面版 · Linux

### 3.1 系统依赖（webkit2gtk-4.1 + libsoup-3.0 + libxdo）

```bash
# Fedora
sudo dnf install webkit2gtk4.1-devel libsoup3-devel libxdo-devel
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libxdo-dev
```

> `libxdo` 为 tao/global-hotkey 链接 `-lxdo` 所需；缺任一库链接会失败。
> 运行时，最终用户机器也需安装 WebKitGTK 运行库（`webkit2gtk-4.1`）。

### 3.2 构建可执行文件

```bash
# 生成样式（§1.3）后：
dx build --platform desktop --release
# 产物（裸可执行 + 资源）：target/dx/eazy-json-view/release/linux/app/
```

或脚本：`./build-desktop.sh build`（含依赖检查 + 样式生成 + 逻辑测试 + 构建）。

### 3.3 打包安装器（.deb / .rpm / .AppImage）

```bash
dx bundle --platform linux --release --package-types deb        # Debian/Ubuntu 包
dx bundle --platform linux --release --package-types rpm        # Fedora/RHEL 包
dx bundle --platform linux --release --package-types appimage   # 通用 AppImage
# 可一次多种：--package-types deb --package-types appimage
```

> AppImage 相关元数据见 `Cargo.toml` 的 `[package.metadata.appimage]`。

---

## 4. 桌面版 · macOS

### 4.1 前置

- **Xcode Command Line Tools**：`xcode-select --install`
- WebView 后端为系统自带 **WKWebView**，**无需安装额外运行时**（最终用户开箱即用）。
- 按芯片添加 Rust 目标（在对应机器上构建）：
  ```bash
  rustup target add aarch64-apple-darwin   # Apple Silicon (M 系列)
  rustup target add x86_64-apple-darwin    # Intel
  ```

### 4.2 构建与打包（.app / .dmg）

```bash
# 生成样式（§1.3，用 macOS 版 tailwind CLI）后：
dx build  --platform macos --release                     # 产出 .app 应用包
dx bundle --platform macos --release --package-types dmg # 产出可分发 .dmg
# 也可 --package-types macos 仅产 .app
```

打包图标取自 `Dioxus.toml` 的 `[bundle] icon = ["assets/icon.png"]`；窗口运行时图标已在 `main.rs` 用 `include_bytes!` 内嵌（跨平台一致）。

### 4.3 分发签名（可选，对外发布才需要）

未签名的 .app/.dmg 在他人机器上会被 Gatekeeper 拦截。对外分发需：

1. Apple Developer 账号 + Developer ID 证书；
2. `codesign` 签名 .app；
3. `notarytool` 公证 + `stapler` 装订。

仅本机自用可跳过（首次打开时右键「打开」绕过）。

### 4.4 通用二进制（可选）

如需同时支持 Intel + Apple Silicon，分别在/为两架构构建后用 `lipo` 合并，或在两类机器上各出一份。

---

## 5. 桌面版 · Windows

### 5.1 前置

- **Visual Studio Build Tools**（含「使用 C++ 的桌面开发」工作负载，提供 MSVC 链接器）。Rust 默认用 `x86_64-pc-windows-msvc` 工具链。
- **WebView2 运行时**：Windows 11 与绝大多数 Windows 10 已预装；若目标机缺失，安装微软「Evergreen WebView2 Runtime」（开发机构建时一般已具备）。
- 在 Windows 机器上构建（PowerShell / CMD）。

### 5.2 生成样式

下载 `tailwindcss-windows-x64.exe`（§1.3），然后：

```powershell
.\tailwindcss-windows-x64.exe -c tailwind.config.js -i assets/input.css -o assets/tailwind.css --minify
```

### 5.3 构建与打包（.msi / .exe）

```powershell
dx build  --platform windows --release                      # 产出 .exe 可执行
dx bundle --platform windows --release --package-types msi  # WiX 生成 .msi 安装包
dx bundle --platform windows --release --package-types nsis # NSIS 生成 .exe 安装器
```

> `msi` 需要 WiX Toolset、`nsis` 需要 NSIS；dx 通常会提示/拉取所需打包工具。若只需绿色免安装版，用 `dx build` 的 `.exe` 即可。

---

## 6. 用 CI 一次产出全部平台（GitHub Actions，推荐）

在三种 runner 上并行构建。下例产出各平台安装包 + Web 静态站点（按需裁剪）：

```yaml
# .github/workflows/build.yml
name: build
on: [push, workflow_dispatch]

jobs:
  desktop:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            platform: linux
            pkg: appimage
          - os: macos-latest
            platform: macos
            pkg: dmg
          - os: windows-latest
            platform: windows
            pkg: msi
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      # Linux 系统依赖
      - if: matrix.os == 'ubuntu-latest'
        run: sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libsoup-3.0-dev libxdo-dev

      - name: Install dx
        run: cargo install dioxus-cli --version 0.7.9 --locked

      # 生成 Tailwind（assets/tailwind.css 若已提交且未改类名，可跳过这步）
      - name: Tailwind (linux)
        if: matrix.os == 'ubuntu-latest'
        run: |
          curl -sSL -o tw https://github.com/tailwindlabs/tailwindcss/releases/download/v3.4.17/tailwindcss-linux-x64
          chmod +x tw && ./tw -c tailwind.config.js -i assets/input.css -o assets/tailwind.css --minify
      # macOS / Windows 同理换用 tailwindcss-macos-arm64 / tailwindcss-windows-x64.exe

      - name: Bundle
        run: dx bundle --platform ${{ matrix.platform }} --release --package-types ${{ matrix.pkg }}

      - uses: actions/upload-artifact@v4
        with:
          name: eazy-json-view-${{ matrix.platform }}
          path: target/dx/eazy-json-view/bundle/   # 按实际产物目录调整

  web:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: wasm32-unknown-unknown }
      - run: cargo install dioxus-cli --version 0.7.9 --locked
      - run: |
          curl -sSL -o tw https://github.com/tailwindlabs/tailwindcss/releases/download/v3.4.17/tailwindcss-linux-x64
          chmod +x tw && ./tw -c tailwind.config.js -i assets/input.css -o assets/tailwind.css --minify
      - run: dx build --platform web --release
      - uses: actions/upload-artifact@v4
        with:
          name: eazy-json-view-web
          path: target/dx/eazy-json-view/release/web/public/
```

> macOS/Windows 的 Tailwind 步骤换用对应 CLI；`upload-artifact` 的 `path` 以本机实际产物路径为准（`dx bundle` 结束时会打印最终文件位置）。

---

## 7. 跨平台注意事项（构建前务必通读）

### 7.1 ⚠️ 文件对话框依赖（rfd）在 Windows/macOS 上的调整

`Cargo.toml` 当前把 `rfd` 固定为 **Linux 专用的 `xdg-portal` 后端**，且作用于**所有原生目标**：

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
rfd = { version = "0.17", default-features = false, features = ["xdg-portal"] }
```

`xdg-portal` 仅在 Linux 生效（其依赖 `ashpd` 按 `cfg(linux)` 门控）。在 macOS/Windows 上通常仍能编译并走系统原生后端，**但为稳妥起见，建议为 Windows/macOS 构建时改成按平台分叉**——把上面这段替换为：

```toml
# 其余原生依赖保持在通用块
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
dirs = "6"
futures-timer = "3"

# Linux：xdg-portal（无 GTK，避免与 wry 的 GTK 主循环冲突）
[target.'cfg(all(not(target_arch = "wasm32"), target_os = "linux"))'.dependencies]
rfd = { version = "0.17", default-features = false, features = ["xdg-portal"] }

# macOS / Windows：用 rfd 默认（系统原生）后端
[target.'cfg(all(not(target_arch = "wasm32"), not(target_os = "linux")))'.dependencies]
rfd = { version = "0.17" }
```

构建后请在每个平台**实测「导入文件」与「下载/另存」对话框**确实能弹出。

### 7.2 release 编译较慢但产物更小

`[profile.release]` 启用了 `lto = true`、`codegen-units = 1`、`panic = "abort"`、`opt-level = 3`。首次 release 构建偏慢属正常。

### 7.3 其它

- **窗口图标**：`main.rs` 用 `include_bytes!("../assets/icon.png")` 内嵌，三平台一致，无运行时路径依赖。
- **i18n**：`locales/*.yml` 编译期内嵌，无运行时文件，三平台/Web 通用。
- **数据存储**：桌面落盘到用户配置目录下单个 `store.json`（`dirs` 定位）；Web 用 `localStorage`。
- **绝不同时开启 `web` 与 `desktop` 两个 renderer 特性**；`dx --platform web` 会自动 `--no-default-features --features web`。

---

## 8. 产物速查

| 目标 | 构建命令 | 产物 |
|------|----------|------|
| Web | `dx build --platform web --release` | `target/dx/eazy-json-view/release/web/public/`（静态站点）|
| Linux | `dx bundle --platform linux --release --package-types deb\|rpm\|appimage` | `.deb` / `.rpm` / `.AppImage` |
| macOS | `dx bundle --platform macos --release --package-types dmg` | `.app` / `.dmg` |
| Windows | `dx bundle --platform windows --release --package-types msi\|nsis` | `.msi` / `.exe` |

> `dx bundle` 完成时会在终端打印最终文件的确切路径，以该输出为准。

---

## 9. 常见问题排错

| 现象 | 原因 / 处理 |
|------|-------------|
| `dx build` 报找不到 `assets/tailwind.css` | 未先生成样式 → 执行 §1.3 |
| Linux 链接报 `-lxdo` / webkit 相关失败 | 缺系统依赖 → §3.1 |
| Web release 末尾 `wasm-opt failed` | 已知现象，非失败，产物可用 → §2 |
| Windows 运行白屏/无界面 | 目标机缺 WebView2 运行时 → 安装 Evergreen Runtime |
| macOS 双击提示「已损坏 / 无法验证开发者」 | 未签名公证；自用右键「打开」，分发需 §4.3 |
| Win/macOS 上「导入/下载」对话框异常 | rfd 后端 → 按 §7.1 调整 |
| 纯逻辑验证（无需 webkit） | `cargo test --lib --no-default-features` |
