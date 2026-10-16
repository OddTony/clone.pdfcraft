#!/usr/bin/env bash
# Build, bundle and sign PdfCraft for iOS and iPadOS (on a Mac with Xcode):
#
#   $CARGO_TARGET_DIR/ios-package/<sdk>/PdfCraft.app   the app bundle
#   $DIST/pdfcraft-<version>-ios.ipa                    device builds only, for sideloading
#
# Usage: packaging/ios/package.sh [--simulator | --device] [--skip-build] [--no-models] [--run]
#
#   --simulator   (default) arm64 iOS Simulator build, ad-hoc signed
#   --device      arm64 device build, signed with IOS_SIGN_IDENTITY and IOS_PROVISIONING_PROFILE
#   --skip-build  bundle what cargo built last
#   --no-models   leave the OCR models out (Recognize Text then says they are missing)
#   --run         install and launch: on the booted simulator, or on IOS_DEVICE (devicectl)
#
# Environment:
#   IOS_BUNDLE_ID            bundle identifier (default ai.storyteller.pdfcraft); a device build
#                            needs one your provisioning profile covers
#   IOS_SIGN_IDENTITY        device builds: codesign identity ("Apple Development: ..." or SHA-1)
#   IOS_PROVISIONING_PROFILE device builds: path to a .mobileprovision for IOS_BUNDLE_ID
#   IOS_DEVICE               --run on a device: its identifier or name (xcrun devicectl list devices)
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/ios"

SDK=iphonesimulator
SKIP_BUILD=0
MODELS=1
RUN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --simulator) SDK=iphonesimulator; shift ;;
    --device) SDK=iphoneos; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --no-models) MODELS=0; shift ;;
    --run) RUN=1; shift ;;
    -h | --help) sed -n '2,21p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$SDK" in
  iphonesimulator) TARGET=aarch64-apple-ios-sim; SUPPORTED_PLATFORM=iPhoneSimulator ;;
  iphoneos) TARGET=aarch64-apple-ios; SUPPORTED_PLATFORM=iPhoneOS ;;
esac

if ! xcrun --sdk "$SDK" --show-sdk-path >/dev/null 2>&1; then
  echo "error: the $SDK SDK was not found; install Xcode and run: sudo xcode-select -s /Applications/Xcode.app" >&2
  exit 1
fi

# Keep in sync with MinimumOSVersion in Info.plist.in.
MIN_IOS=15.0
export IPHONEOS_DEPLOYMENT_TARGET="$MIN_IOS"
BUNDLE_ID="${IOS_BUNDLE_ID:-ai.storyteller.pdfcraft}"
SHORT_VERSION="${VERSION%%-*}"
WORK="$CARGO_TARGET_DIR/ios-package/$SDK"
APP="$WORK/PdfCraft.app"

if [ "$SDK" = iphoneos ]; then
  : "${IOS_SIGN_IDENTITY:?a device build needs IOS_SIGN_IDENTITY (security find-identity -v -p codesigning)}"
  : "${IOS_PROVISIONING_PROFILE:?a device build needs IOS_PROVISIONING_PROFILE (a .mobileprovision for $BUNDLE_ID)}"
  [ -f "$IOS_PROVISIONING_PROFILE" ] || { echo "error: no such profile: $IOS_PROVISIONING_PROFILE" >&2; exit 1; }
fi

echo "==> PdfCraft $VERSION for iOS ($SDK, $TARGET), bundle id $BUNDLE_ID"

# ---- build -------------------------------------------------------------------------------------
if [ "$SKIP_BUILD" = 0 ]; then
  rustup target add "$TARGET" >/dev/null
  (cd "$ROOT" && cargo build --release --locked -p pdfcraft --target "$TARGET")
fi
BIN="$CARGO_TARGET_DIR/$TARGET/release/pdfcraft"
[ -x "$BIN" ] || { echo "error: no $BIN; build without --skip-build" >&2; exit 1; }

# ---- PdfCraft.app ------------------------------------------------------------------------------
# iOS bundles are flat: the executable, Info.plist and resources side by side.
echo "==> assembling $APP"
rm -rf "$WORK"
mkdir -p "$APP"
cp "$BIN" "$APP/PdfCraft"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" \
  -e "s/@BUILD_SHA@/${PDFCRAFT_BUILD_SHA:-unknown}/g" -e "s/@BUNDLE_ID@/$BUNDLE_ID/g" \
  -e "s/@PLATFORM@/$SDK/g" -e "s/@SUPPORTED_PLATFORM@/$SUPPORTED_PLATFORM/g" -e "s/@MIN_IOS@/$MIN_IOS/g" \
  "$HERE/Info.plist.in" >"$APP/Info.plist"
printf 'APPL????' >"$APP/PkgInfo"

# App icon: a one-size (1024 pt) asset catalogue compiled into Assets.car, as Xcode does.
ICONS="$WORK/Assets.xcassets"
mkdir -p "$ICONS/AppIcon.appiconset"
cp "$ROOT/assets/app-icon/pdfcraft-1024.png" "$ICONS/AppIcon.appiconset/AppIcon.png"
cat >"$ICONS/Contents.json" <<'JSON'
{ "info" : { "author" : "xcode", "version" : 1 } }
JSON
cat >"$ICONS/AppIcon.appiconset/Contents.json" <<'JSON'
{
  "images" : [ { "filename" : "AppIcon.png", "idiom" : "universal", "platform" : "ios", "size" : "1024x1024" } ],
  "info" : { "author" : "xcode", "version" : 1 }
}
JSON
xcrun actool --compile "$APP" --platform "$SDK" --minimum-deployment-target "$MIN_IOS" \
  --target-device iphone --target-device ipad --app-icon AppIcon \
  --output-partial-info-plist "$WORK/icon-info.plist" --output-format human-readable-text "$ICONS"
# actool's CFBundleIcons / CFBundleIcons~ipad entries (Info.plist.in names the icon set only).
/usr/libexec/PlistBuddy -c "Delete :CFBundleIcons" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Merge $WORK/icon-info.plist" "$APP/Info.plist"
plutil -lint "$APP/Info.plist"

copy_font_licences "$APP"
for f in LICENSE-MIT LICENSE-APACHE NOTICE; do cp "$ROOT/$f" "$APP/"; done
# OCR models: pdfcraft_ocr::Models::dirs_beside_exe looks in `models/` beside the executable.
if [ "$MODELS" = 1 ]; then
  stage_models "$APP/models"
else
  warn "iOS: built without OCR models (--no-models)"
fi

# ---- signing -----------------------------------------------------------------------------------
if [ "$SDK" = iphoneos ]; then
  cp "$IOS_PROVISIONING_PROFILE" "$APP/embedded.mobileprovision"
  security cms -D -i "$IOS_PROVISIONING_PROFILE" >"$WORK/profile.plist"
  plutil -extract Entitlements xml1 -o "$WORK/entitlements.plist" "$WORK/profile.plist"
  APP_ID="$(plutil -extract application-identifier raw -o - "$WORK/entitlements.plist")"
  case "$APP_ID" in
    *."$BUNDLE_ID" | *.\*) ;;
    *) echo "error: the profile is for $APP_ID, not $BUNDLE_ID; set IOS_BUNDLE_ID to match it" >&2; exit 1 ;;
  esac
  # A wildcard profile signs this bundle id: name it in the entitlements.
  TEAM="${APP_ID%%.*}"
  plutil -replace application-identifier -string "$TEAM.$BUNDLE_ID" "$WORK/entitlements.plist"
  codesign --force --sign "$IOS_SIGN_IDENTITY" --entitlements "$WORK/entitlements.plist" --timestamp=none "$APP"
else
  codesign --force --sign - --timestamp=none "$APP"
fi
codesign --verify --strict --verbose=2 "$APP"

if [ "$SDK" = iphoneos ]; then
  IPA="$DIST/pdfcraft-$VERSION-ios.ipa"
  rm -rf "$WORK/Payload" "$IPA"
  mkdir -p "$WORK/Payload"
  ditto "$APP" "$WORK/Payload/PdfCraft.app"
  (cd "$WORK" && ditto -c -k --keepParent Payload "$IPA")
  ls -lh "$IPA"
fi

# ---- run ---------------------------------------------------------------------------------------
if [ "$RUN" = 1 ]; then
  if [ "$SDK" = iphonesimulator ]; then
    if ! xcrun simctl list devices booted | grep -q Booted; then
      echo "error: no simulator is booted; open Simulator.app (or xcrun simctl boot <device>) first" >&2
      exit 1
    fi
    xcrun simctl install booted "$APP"
    # Standard output and error (PdfCraft's log) stream here until the app quits.
    xcrun simctl launch --console-pty --terminate-running-process booted "$BUNDLE_ID"
  else
    : "${IOS_DEVICE:?--run on a device needs IOS_DEVICE (xcrun devicectl list devices)}"
    xcrun devicectl device install app --device "$IOS_DEVICE" "$APP"
    xcrun devicectl device process launch --console --terminate-existing --device "$IOS_DEVICE" "$BUNDLE_ID"
  fi
fi
echo "==> done: $APP"
