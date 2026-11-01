#!/bin/sh
# Compiles AppIcon.icon (Icon Composer) into Assets.car, the Liquid Glass icon
# bundle.icon ships. Tauri can compile the .icon itself, but its actool step
# crashes on Xcode 27 (tauri-apps/tauri#16386), so the compiled file is checked
# in instead. Rerun after editing AppIcon.icon; needs Xcode 26 or later.
set -eu
cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp -R AppIcon.icon "$work/Icon.icon"
mkdir "$work/out"
killall ibtoold 2>/dev/null || true
xcrun actool "$work/Icon.icon" --compile "$work/out" \
  --output-format human-readable-text --errors \
  --output-partial-info-plist "$work/info.plist" \
  --app-icon Icon --include-all-app-icons --enable-on-demand-resources NO \
  --development-region en --target-device mac \
  --minimum-deployment-target 26.0 --platform macosx </dev/null
cp "$work/out/Assets.car" Assets.car
