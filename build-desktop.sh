#!/bin/bash
# Easy Json View 桌面版构建/运行脚本（dx + 原生窗口，无 Python、无浏览器）。
# 用法：
#   ./build-desktop.sh          # 生成样式 → 逻辑测试 → dx serve（弹出原生窗口，阻塞）
#   ./build-desktop.sh build    # 仅构建可执行产物（非阻塞）
set -e
cd "$(dirname "$0")"

MODE="${1:-serve}"

echo "🖥  Easy Json View 桌面版（mode=$MODE）"

# 0. 系统依赖检查：Dioxus 0.7 桌面端（wry/tao）需
#    webkit2gtk-4.1 + libsoup-3.0（webview），以及 libxdo（tao/global-hotkey 链接所需）。
if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  echo "❌ 缺少 webkit2gtk-4.1。请先安装系统依赖（需 root）："
  echo "     sudo dnf install webkit2gtk4.1-devel libsoup3-devel libxdo-devel   # Fedora"
  echo "     sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libxdo-dev  # Debian/Ubuntu"
  exit 1
fi
if ! ldconfig -p 2>/dev/null | grep -q 'libxdo\.so'; then
  echo "❌ 缺少 libxdo（链接 -lxdo 失败）。请安装："
  echo "     sudo dnf install libxdo-devel   # Fedora"
  echo "     sudo apt install libxdo-dev     # Debian/Ubuntu"
  exit 1
fi

# 1. 生成离线 Tailwind CSS（asset! 在编译期要求 assets/tailwind.css 存在）
TW=./tailwindcss
if [ ! -x "$TW" ]; then
  echo "📦 下载 Tailwind v3 standalone CLI..."
  curl -sSL -o "$TW" "https://github.com/tailwindlabs/tailwindcss/releases/download/v3.4.17/tailwindcss-linux-x64"
  chmod +x "$TW"
fi
echo "🎨 生成 assets/tailwind.css..."
"$TW" -c tailwind.config.js -i assets/input.css -o assets/tailwind.css --minify

# 2. 逻辑测试（纯 services 层，作为构建前置门禁）
echo "🧪 运行逻辑测试..."
cargo test --lib

# 3. 构建 / 运行桌面端（默认平台来自 Cargo features=desktop，--platform 可省略）
if [ "$MODE" = "build" ]; then
  echo "🔨 dx build --platform desktop ..."
  dx build --platform desktop
  echo "✅ 构建完成。"
else
  echo "🚀 dx serve --platform desktop（弹出原生窗口；Ctrl+C 退出）..."
  dx serve --platform desktop
fi
