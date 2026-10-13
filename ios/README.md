# aRTI-PDF iOS companion app

A SwiftUI viewer for PDFs, backed directly by the native aRTI-PDF Rust engine
(`apps/pdfcraft-mobile-ffi`) through a UniFFI-generated Swift binding. No PDF
parsing/rendering logic lives in Swift — it's the same clean-room engine used
by the desktop app, just called from the other side of an FFI boundary.

## Scope (MVP)

- Open a PDF via the system document picker.
- Render pages through the engine and page through the document.
- Simulator-only builds in CI. No code signing, no device builds, no
  App Store distribution yet.

Editing, saving, and the rest of the desktop feature set come later once the
viewer pipeline is proven out in CI.

## Layout

- `project.yml` — [XcodeGen](https://github.com/yonaskolb/XcodeGen) project
  spec. Run `xcodegen generate` after the build script below to produce
  `ArtiPDF.xcodeproj`.
- `Sources/App` — hand-written SwiftUI sources.
- `Sources/Generated` — **gitignored.** Populated by
  `Scripts/build-ffi.sh`, which runs `uniffi-bindgen` to produce
  `pdfcraft_mobile_ffi.swift` from the Rust crate.
- `Frameworks/pdfcraft_mobile_ffiFFI.xcframework` — **gitignored.** Built by
  the same script: a static library + C header/modulemap pair wrapped as an
  XCFramework so Xcode can resolve the bindings' `import pdfcraft_mobile_ffiFFI`.
- `Scripts/build-ffi.sh` — builds the Rust staticlib for the iOS Simulator
  target, generates the Swift bindings, and assembles the XCFramework. Run
  this before `xcodegen generate`.

## Building locally (requires macOS + Xcode)

```sh
cd ios
./Scripts/build-ffi.sh
xcodegen generate
xcodebuild build \
  -project ArtiPDF.xcodeproj -scheme ArtiPDF \
  -destination 'platform=iOS Simulator,name=iPhone 15'
```

## CI

`.github/workflows/ios.yml` runs the same sequence on a macOS runner and
builds for the Simulator. There is no local macOS machine in this project's
usual dev environment, so GitHub Actions is the only way to validate iOS
changes — expect to iterate based on CI failures.
