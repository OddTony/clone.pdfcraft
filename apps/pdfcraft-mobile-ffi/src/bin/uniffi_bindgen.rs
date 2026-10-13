//! `cargo run --bin uniffi-bindgen -- generate ...` entry point: generates the Swift bindings
//! for the `pdfcraft_mobile_ffi` library from its proc-macro scaffolding. Only built with the
//! `cli` feature (see `Cargo.toml`), which pulls in `uniffi`'s bindgen CLI machinery that the
//! mobile app itself does not need at runtime.

fn main() {
    uniffi::uniffi_bindgen_main();
}
