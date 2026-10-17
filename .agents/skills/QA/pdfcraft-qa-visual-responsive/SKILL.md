---
name: pdfcraft-qa-visual-responsive
description: Use when PdfCraft layout, typography, dialogs, page presentation, scaling, or hit geometry changes across window sizes or rendering environments.
---

# Visual and responsive evidence

Use for visible layout/transition claim. Pure document semantics without presentation change: non-trigger. Read [task contract](references/contract.md), [UI/platform reference](references/ui-platforms.md), and [evidence guidance](references/evidence.md). Identify changed view, approved design/baseline, relevant states, actual logical size/scale boundaries, supported backend, allowed artifacts, resource owner, finite matrix. Recheck current screenshot/font/asset rules before capture.

Define visual behavior independently of pixel matching: visible controls/focus, legible labels, dialogs fit or expose intended scrolling, correct page geometry, hit regions match drawn positions. Choose sizes immediately around actual layout changes plus wider control. Include affected long translations, missing-font behavior, validation errors, high zoom, resize with active menu/dialog/selection then return. Two fresh states can miss transition defect.

Use deterministic egui/kittest geometry/snapshots when question reachable. Pin logical size, pixels per point, theme, locale, font/build inputs, cursor, animations/clock, renderer/GPU, OS, harness features. Record pre-capture state if capture/animation stabilization could change it. Small window supplies no touch, physical DPI change or native-dialog evidence. `with_os` changes egui data; cannot exercise another platform backend.

Example: synthetic PDF, translated validation dialog; resize across wrapping boundary, Tab to final action; verify focus visibility/click-target bounds. Keep baseline/current/diff images, raw geometry. Appropriate visual/interaction assertion must detect deliberate clipped final action or shifted hit rectangle even with most pixels unchanged. Review every material difference against requirement before separately authorized baseline update; never mask changed region or loosen tolerances for green.

Real monitor movement, compositor scaling, rfd pickers, physical input, browser-WASM require corresponding native/browser run. Preserve per-environment baselines or justified geometric tolerances; cross-OS pixel identity not universal gate. Publication screenshots require approved visible assets and `PDFCRAFT_SYSTEM_FONTS=0`.

Absent matching GPU/fonts/app/backend: inspect source, prepare bounded cases; rendering qualification incomplete. No snapshot service installation, unapproved server or document upload. Retain source/build/fixture identity, selected/executed states, actual capture settings, first diffs, review decisions, cleanup, remaining platform limits.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
