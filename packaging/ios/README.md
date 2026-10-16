# PdfCraft for iOS and iPadOS

The desktop app (`apps/pdfcraft`), built for iOS: the same egui interface, drawn with wgpu on
Metal, with the native document picker and alerts standing in for desktop file dialogs
(`vendor/rfd`, PdfCraft patch 1).

**Status: experimental.** The whole app type-checks for `aarch64-apple-ios` and
`aarch64-apple-ios-sim`, but it has not yet been run on a simulator or a device.

## Build and run

On a Mac with Xcode (15 or later) and rustup:

```sh
# Simulator (ad-hoc signed). Boot one first: open -a Simulator
packaging/ios/package.sh --simulator --run

# Device: a development certificate and a provisioning profile for your bundle id
IOS_BUNDLE_ID=com.example.pdfcraft \
IOS_SIGN_IDENTITY="Apple Development: Your Name (TEAMID)" \
IOS_PROVISIONING_PROFILE=~/Downloads/PdfCraft_Dev.mobileprovision \
IOS_DEVICE="Your iPad" \
packaging/ios/package.sh --device --run
```

`--no-models` skips downloading the OCR models (Recognize Text then reports them missing).
A device build also writes `dist/release/pdfcraft-<version>-ios.ipa`. `--help` lists every
option. The minimum system is iOS 15.

## How it maps onto iOS

| Desktop | iOS |
|---|---|
| Open panel | Document picker. The picked file is copied into the app's `Documents` folder, which the Files app shows as On My iPad ▸ PdfCraft. Save writes that copy. |
| Save panel | A file-name prompt, then a folder picker that starts in `Documents`. |
| Folder panel | Folder picker. Access to the folder lasts until the app quits. |
| Message boxes | Alerts. |
| Settings, logs, crash recovery | `Library/Application Support/PdfCraft` in the app's container. |
| Clipboard image cache | `Library/Caches/PdfCraft/Clipboard`. |
| Interface language (Auto) | Settings ▸ General ▸ Language & Region. |
| OpenGL fallback (`PDFCRAFT_RENDERER=gl`) | None: wgpu on Metal only. |

## Known gaps

- **Not yet tested on a simulator or device**, so expect runtime problems the type checker
  can't catch.
- **The interface is the desktop one.** It is usable on an iPad, especially with a keyboard and
  trackpad, and cramped on an iPhone. Touch scrolling, tapping and pinch zoom go through egui's
  touch support. The on-screen keyboard appears when a text field takes focus.
- **No "Open in PdfCraft" from Files or the share sheet.** winit 0.30 doesn't pass on the URLs
  iOS opens an app with, so the bundle declares no document types. Use File ▸ Open.
- **Picked files are copies.** Saving changes the copy in `Documents`, not the original in
  iCloud Drive or another app. Use Save As and pick the original folder to put it back.
- **Desktop-only features** (printing, signing with Keychain identities, Reveal in Finder, the
  update check, extra system fonts for the interface) are still in the menus and fail or do
  nothing on iOS. Build with `CRAFT_FONTS_DIR` for Japanese interface text.
