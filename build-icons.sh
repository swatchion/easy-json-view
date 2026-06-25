#!/usr/bin/env bash
# 由 assets/icon.svg 重生成各尺寸应用图标 PNG（透明底 8-bit RGBA）。
#
# - Dioxus.toml [bundle].icon 引用固定文件名 icon-{32,48,64,128,256,512}.png（打包图标）。
# - main.rs 的窗口图标 include_bytes! assets/icon.png（256 尺寸）。
# 改了 assets/icon.svg 后须重跑本脚本，三平台图标方随之更新（矢量仅存在于 SVG 源层，发栅格）。
#
# 依赖：ImageMagick（magick）。-background none 保透明底；PNG32 + -depth 8 强制 8-bit RGBA
# （.deb 打包要求逐尺寸 8-bit PNG）。
set -euo pipefail
cd "$(dirname "$0")"

SVG="assets/icon.svg"
command -v magick >/dev/null 2>&1 || {
  echo "需要 ImageMagick（magick）。Fedora: sudo dnf install ImageMagick" >&2
  exit 1
}

for n in 32 48 64 128 256 512; do
  magick -background none "$SVG" -resize "${n}x${n}" -depth 8 "PNG32:assets/icon-${n}.png"
  echo "生成 assets/icon-${n}.png"
done

# main.rs 窗口图标（include_bytes! assets/icon.png）：用 256 尺寸。
magick -background none "$SVG" -resize 256x256 -depth 8 "PNG32:assets/icon.png"
echo "生成 assets/icon.png (256)"
