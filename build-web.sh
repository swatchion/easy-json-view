#!/bin/bash
# Easy Json View Web 版构建脚本（dx + WASM，输出可静态托管的产物）。
# 用法：
#   ./build-web.sh          # 生成样式 → dx build --platform web → 逻辑测试
#   ./build-web.sh serve    # 本地预览：dx serve --platform web（阻塞）
set -e
cd "$(dirname "$0")"

MODE="${1:-build}"

echo "🌐 Easy Json View Web 版（mode=$MODE）"

# 1. 生成离线 Tailwind CSS（不再使用 CDN）
TW=./tailwindcss
if [ ! -x "$TW" ]; then
  echo "📦 下载 Tailwind v3 standalone CLI..."
  curl -sSL -o "$TW" "https://github.com/tailwindlabs/tailwindcss/releases/download/v3.4.17/tailwindcss-linux-x64"
  chmod +x "$TW"
fi
echo "🎨 生成 assets/tailwind.css..."
"$TW" -c tailwind.config.js -i assets/input.css -o assets/tailwind.css --minify

# 2. 确保已添加 WASM 目标
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

if [ "$MODE" = "serve" ]; then
  echo "🚀 dx serve --platform web（本地预览；Ctrl+C 退出）..."
  dx serve --platform web
  exit 0
fi

# 3. 构建 Web 产物
echo "🔨 dx build --platform web --release ..."
dx build --platform web --release

# 4. 逻辑测试（--no-default-features 避开桌面 renderer，无需 webkit 即可在任意机器运行）
echo "🧪 运行逻辑测试..."
cargo test --lib --no-default-features

echo "✅ 构建完成。静态产物位于 dx 的 web 输出目录："
echo "     target/dx/easy-json-view/release/web/public/"
echo "   可用任意静态服务器托管，或 './build-web.sh serve' 直接本地预览。"
