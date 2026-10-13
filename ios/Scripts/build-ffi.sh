#!/usr/bin/env bash
# Builds the Rust FFI staticlib for the iOS Simulator, generates UniFFI Swift
# bindings, and assembles everything into an XCFramework that Xcode can link
# against. Simulator-only (MVP scope) — see ios/README.md.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ios_dir="$repo_root/ios"
crate_dir="$repo_root/apps/pdfcraft-mobile-ffi"
target="aarch64-apple-ios-sim"
crate_name="pdfcraft_mobile_ffi"
module_name="${crate_name}FFI"

build_dir="$ios_dir/build"
headers_dir="$build_dir/headers"
generated_dir="$ios_dir/Sources/Generated"
frameworks_dir="$ios_dir/Frameworks"
xcframework_path="$frameworks_dir/${module_name}.xcframework"

echo "==> Installing Rust target: $target"
rustup target add "$target"

echo "==> Building $crate_name (release, $target)"
cargo build \
  --manifest-path "$crate_dir/Cargo.toml" \
  --release \
  --target "$target"

staticlib="$repo_root/target/$target/release/lib${crate_name}.a"
if [[ ! -f "$staticlib" ]]; then
  echo "error: expected staticlib not found at $staticlib" >&2
  exit 1
fi

echo "==> Generating Swift bindings"
rm -rf "$generated_dir"
mkdir -p "$generated_dir"
cargo run \
  --manifest-path "$crate_dir/Cargo.toml" \
  --features cli \
  --bin uniffi-bindgen -- \
  generate \
  --library "$staticlib" \
  --language swift \
  --out-dir "$generated_dir"

echo "==> Assembling Clang module for $module_name"
rm -rf "$headers_dir"
mkdir -p "$headers_dir"

# uniffi-bindgen writes the C header + modulemap alongside the .swift file;
# move them out so Sources/Generated only contains Swift app sources, and
# rename the modulemap to the name Xcode expects.
shopt -s nullglob
header_files=("$generated_dir"/*.h)
modulemap_files=("$generated_dir"/*.modulemap)
shopt -u nullglob

if [[ ${#header_files[@]} -eq 0 || ${#modulemap_files[@]} -eq 0 ]]; then
  echo "error: uniffi-bindgen did not produce the expected .h/.modulemap files" >&2
  exit 1
fi

mv "${header_files[@]}" "$headers_dir/"
mv "${modulemap_files[0]}" "$headers_dir/module.modulemap"

echo "==> Creating XCFramework at $xcframework_path"
rm -rf "$xcframework_path"
mkdir -p "$frameworks_dir"
xcodebuild -create-xcframework \
  -library "$staticlib" \
  -headers "$headers_dir" \
  -output "$xcframework_path"

echo "==> Done. Swift bindings in $generated_dir, framework at $xcframework_path"
