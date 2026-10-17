# UI, locale, and platform evidence

Choose evidence at claim's crossed boundary. Baseline source f0c9901c398372782a4eb001c95638289a5edab0 locks egui/eframe/egui_kittest 0.36.2, kittest 0.4.0, AccessKit 0.24.1, rfd 0.17.2, vendored winit 0.30.13, wasm-bindgen 0.2.129. Recheck Cargo.lock and enabled features before run; available documentation/dependency declaration cannot prove compiled result.

## Evidence layers

| Claim | Useful evidence | Remaining limit |
|---|---|---|
| Command/precondition/domain state | Engine/automation tests, registry checks | Cannot establish keyboard/visual reachability |
| Egui control semantics | kittest/AccessKit role, name, value/state, focus, activation, independent app state | Cannot establish OS accessibility events, reader speech, real dialog, monitor or IME |
| Rendered shell/geometry | Environment-pinned GPU snapshot, measured bounds, interaction hit testing | Cannot establish semantic reading order, speech, physical input or other backend |
| Native integration | Real app/window, actual OS/backend, reader, picker, IME, monitor, keyboard | harness.with_os or another OS's run cannot substitute |
| Browser-WASM | Actual built artifact in recorded browser/OS; open/edit/render/download/host tests | Compile checks/native UI tests do not execute path |
| PDF accessibility | Document structure/value/save/reopen plus independent viewer/AT/manual review | App AccessKit/identical pixels cannot prove PDF tags/meaningful descriptions |

Use smallest adequate layer; retain gaps. Low-layer pass may coexist with unavailable native behavior. Complete collection receipt proves collection only; missing, wrong-scope, incomplete or empty capture cannot prove absent behavior.

## Egui and input recipes

Follow pinned `crates/ui-egui/tests/control.rs` idiom: `Harness::builder().with_size(...).with_pixels_per_point(...).build_eframe(...)` receives app directly from closure in this version. Existing tests attach control to egui context, open tiny synthetic bytes. `run_steps`: known synchronous transition; worker/dialog preparation needs state predicate/hard deadline. Harness step-limit/error/None cannot settle pass. Test-only panicking assertions do not authorize product panicking input paths.

Keyboard: reachable state, then Tab/Shift-Tab/Enter/Escape or appropriate shortcut. Programmatic focus: setup, no traversal proof. Assert one intended effect plus forbidden duplicate/unrelated effects. Exercise modal focus entry, containment, validation, cancellation, opener removal, focus restoration. Text input may own undo shortcuts separately from document history. Query missing, ambiguous, disabled, nonfocusable controls as negative cases.

Already built native app may use explicitly enabled control channel and `pdfcraft-cli ui` inspect/key/screenshot commands. Recheck actual help/key=value syntax. Control: opt-in, loopback/token protected; do not publish file/token or enable implicitly. Protocol replies, semantic snapshots, screenshots and reader observations: separate artifacts.

Native AT: record app/build, OS/backend, reader version, operator, exact action/state, spoken role/name/value, event timing, errors, coverage. Windows needs actual UI Automation/Narrator session; macOS NSAccessibility/VoiceOver; Linux X11/Wayland respective compositor/input stack and AT-SPI/Orca; FreeBSD actual configured desktop/adapter/reader. Existing Windows/Linux CI or Unix adapter support cannot prove interactive result. Native computer access may be unavailable even on Windows host. Do not add SDK/reader integration merely to fill ledger.

## Locales, fonts, and text

Source registry: twenty-three codes: en, ja, zh-hans, zh-hant, cs, pt-br, de, es, fr, ru, kk, bg, te, hu, uk, it, ar, pl, sl, el, tr, lt, vi. English: source/fallback. Keys distinguish context, command-id and plurals. Test duplicate/unknown contexts, escaping, placeholders, ellipsis, plural arity, independently reviewed plural wording. Logical translations need not be byte-identical. Explicit language validation, persisted preference fallback, system Auto detection: separate paths. Tests force system English; current WASM system detection returns English too. Neither proves native display-language detection.

Switch language on open dirty document; compare command IDs, content, path, history, dirty state; check visible menu/palette/tooltips/dialogs. Test invalid explicit language without mutation. Existing language-switch subset cannot prove all-locale review.

Separate logical storage, grapheme boundaries, glyph presence, shaping, bidi layout, selection/caret, IME composition. Use precomposed/decomposed accents and affected Arabic/Hebrew+Latin/numeric strings; observe compose/commit/cancel and focus transitions on actual input methods. Injected keys cannot reproduce physical composition. Current Arabic docs describe unmirrored layout, wrapping, editing limits; catalogue completeness cannot remove them.

Fonts: approved bundled faces or optional pinned craft-fonts build input. Missing Japanese/CJK/Arabic/Telugu faces may produce boxes; record exact font input/glyph/shaping outcome. Installed runtime fallback font: UI-only; never copy/embed into PDF/package/repository or published screenshot. New fonts follow craft-fonts policy. PDF field `/DA`/appearance fonts: another path; UI glyph coverage cannot prove safe field flattening. Translation TSV changes require attribution hash maintenance under existing asset policy.

## Visual/native/browser boundaries

Pin logical size, pixels per point, DPI/zoom, theme, language, font/build inputs, animations/clock, cursor, GPU/renderer, OS, capture settings. Check actual wrapping thresholds, small windows, longest affected labels, focus/error visibility, hit bounds, resize during active interaction. Preserve baseline/current/diff/raw geometry; review material differences before authorized baseline update. Cross-OS pixel identity: no general gate. Published PdfCraft screenshots require lawful visible assets and `PDFCRAFT_SYSTEM_FONTS=0`.

Real rfd dialog cancellation/overwrite/parent/focus, Microsoft/Apple/IBus/fcitx composition, mixed-monitor scaling, X11 DPI, Wayland seat/compositor, native GPU paths need own environments. Dialog overrides prove response handling, no picker proof. Vendor-patch source-presence guards: static evidence only.

Browser qualification: app/wasm/glue hashes, browser/OS/version, renderer, permissions, scale, locale, fixtures, profile. Use real existing authorized app, synthetic open/edit/render/save, actual downloaded artifact/reopen, blocked/canceled delivery, reload after user activation, host-origin/refusal controls. Blob/anchor or postMessage dispatch does not establish storage acknowledgment. beforeunload: unreliable recovery. Native `accesskit` differs from eframe's `web_screen_reader` feature/options; inspect effective feature closure and actual browser AT; manifest text alone cannot establish runtime defect. wasm-bindgen-test: no currently wired suite; no automatic runner/driver installation.

Source anchors: [control tests](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/ui-egui/tests/control.rs), [localization registry](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/ui-egui/src/i18n/mod.rs), [font/localization limits](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/docs/localization.md), [egui_kittest 0.36.2 Harness](https://docs.rs/egui_kittest/0.36.2/egui_kittest/struct.Harness.html), and [WCAG 2.2](https://www.w3.org/TR/2024/REC-WCAG22-20241212/). Focused checks cannot prove full WCAG/PDF/UA conformance.
