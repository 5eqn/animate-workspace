#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != Darwin ]]; then
  echo 'Run this script on macOS.' >&2
  exit 1
fi
cargo build --release --locked
app_path="target/MORI.app"
mkdir -p "$app_path/Contents/MacOS" "$app_path/Contents/Resources"
cp target/release/mori "$app_path/Contents/MacOS/MORI"
# Finder launches do not inherit an interactive shell PATH. Bundle FFmpeg when available.
if command -v ffmpeg >/dev/null; then
  cp "$(command -v ffmpeg)" "$app_path/Contents/MacOS/ffmpeg"
fi
iconset_path="target/MORI.iconset"
mkdir -p "$iconset_path"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/icon.png --out "$iconset_path/icon_${size}x${size}.png" >/dev/null
  double_size=$((size * 2))
  sips -z "$double_size" "$double_size" assets/icon.png --out "$iconset_path/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset_path" -o "$app_path/Contents/Resources/MORI.icns"
cat > "$app_path/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIconFile</key><string>MORI.icns</string>
<key>CFBundleName</key><string>MORI</string>
<key>CFBundleDisplayName</key><string>MORI</string>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleLocalizations</key><array><string>en</string><string>zh-Hans</string></array>
<key>CFBundleIdentifier</key><string>studio.mori.animation</string>
<key>CFBundleExecutable</key><string>MORI</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSSupportsAutomaticGraphicsSwitching</key><true/>
<key>LSMinimumSystemVersion</key><string>11.0</string>
</dict></plist>
PLIST
codesign --force --deep --sign - "$app_path"
echo "Built: $app_path"
