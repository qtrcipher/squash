# Squash — 04: Design Direction

> Status: decision document. This is THE direction — not options. Inputs: `docs/01-product-scope.md`, `docs/02-market-check.md`.
> Method: `ui-ux-pro-max` skill database (`--design-system` + `style`/`typography` domain queries).
> House rules honored: EN + AR (full RTL), light + dark, native feel per OS, all strings i18n'd.

## 1. Style direction

**Chosen: Flat Design (database: "Flat Design", styles.csv) — executed as *platform-native flat*.**

Flat, border-defined, zero decorative chrome, one accent color, information-dense. Rationale:
- Squash is a **utility**, not a destination — users are mid-task and want the app to disappear into the OS. Flat + borders reads as "system tool" on all three OSes; the skill rates it WCAG AAA and top performance.
- **Trust is the differentiator** (vs WinRAR nagware, Bandizip ads — see market check §3). A restrained, honest UI with no skeuomorphic gloss, no gradients, no decoration signals "no gimmicks."
- **Density:** queue rows, per-file progress, ratios and ETAs are data-heavy; flat with 1px borders and tight spacing handles density without visual noise.
- **Native-per-OS within one system:** same tokens everywhere, but surfaces adopt platform conventions — vibrancy/material window on macOS, Mica-aware window + Segoe-fluent controls on Windows 11, Adwaita-compatible header bar on Linux/GTK. Components stay flat inside; only window chrome follows the OS.

Rejected:
- **Glassmorphism** — translucency + blur fights three different windowing systems, tanks contrast (AA risk) and cheapens trust; decoration without information value.
- **Neumorphism (Soft UI)** — database marks it light-only, "breaks in dark mode"; dual-shadow extrusion is unmaintainable across toolkits and reads as novelty, not a serious tool.

## 2. Color palette

Single blue accent (trust, technical) + amber reserved for warnings/CTA-warm states. "Folder blue + file amber" per database palette, adapted. All text/background pairs below meet **WCAG AA (≥ 4.5:1 normal text, ≥ 3:1 large/UI)**.

| Token | Light | Dark |
|---|---|---|
| Primary (accent, focus ring, links) | `#2563EB` (on white: 5.2:1) | `#60A5FA` (on `#0F172A`: 7.0:1) |
| Primary fill (buttons) | `#2563EB`, hover `#1D4ED8` | `#3B82F6`, hover `#2563EB` |
| On-primary text | `#FFFFFF` | `#FFFFFF` (bold ≥14px; for small text use `#0F172A` on `#60A5FA`) |
| Window background | `#F8FAFC` | `#0F172A` |
| Surface (cards, queue rows) | `#FFFFFF` | `#1E293B` |
| Surface raised (popovers, dialogs) | `#FFFFFF` | `#273549` |
| Border / separator | `#E2E8F0` | `#334155` |
| Text primary | `#0F172A` | `#F1F5F9` |
| Text secondary | `#475569` | `#94A3B8` |
| Text disabled | `#94A3B8` | `#64748B` |
| Success | `#15803D` | `#4ADE80` |
| Warning | `#B45309` | `#FBBF24` |
| Error / destructive | `#DC2626` | `#F87171` |
| Selection background | `#DBEAFE` | `#1D4ED8` @ 40% |

**Job/progress state colors** (used in progress bars, queue badges, row icons — never color alone, always paired with icon or label):

| State | Light | Dark |
|---|---|---|
| Queued | `#64748B` | `#94A3B8` |
| Compressing / extracting (active) | `#2563EB` | `#60A5FA` |
| Paused | `#B45309` | `#FBBF24` |
| Done | `#15803D` | `#4ADE80` |
| Failed | `#DC2626` | `#F87171` |
| Skipped (protected path, e.g. zip-slip block) | `#7C3AED` | `#A78BFA` |

Theme follows OS by default with an in-app override; both themes are first-class (WinRAR only got dark mode in 2024 — ship both from day one).

## 3. Typography

**Decision: system-font stacks per OS for UI text; Noto Sans Arabic bundled as Arabic fallback; JetBrains Mono bundled for mono.** Rationale: native feel and zero font payload beat brand typography for a utility; system stacks automatically get correct Arabic shaping and metrics per platform, and Linux Arabic coverage is the only real gap — hence the bundled Noto fallback. A custom UI font (Inter) was considered and rejected: it is Latin-centric, adds payload, and breaks the "feels like the OS" promise.

- UI sans (EN): `-apple-system` (SF Pro) / `"Segoe UI Variable Text"` / `Cantarell, "Noto Sans", Ubuntu, system-ui`
- UI sans (AR): system Arabic (SF Arabic / Segoe UI Arabic / system default) → fallback `"Noto Sans Arabic"` (bundled, OFL)
- Mono (paths, sizes, hashes, ETA numbers, CLI echoes): `"JetBrains Mono"` (bundled, OFL) → `ui-monospace, "SF Mono", "Cascadia Mono", "Noto Sans Mono", monospace`. Paths/CLI snippets stay LTR with `dir="ltr"` even in RTL layout.

Type scale (px, desktop utility density; base 13):

| Token | Size / weight / line-height | Use |
|---|---|---|
| Caption | 11 / 400 / 1.35 | badges, secondary metadata |
| Body-small | 12 / 400 / 1.4 | queue row metadata, mono paths |
| Body | 13 / 400 / 1.45 | default UI text, table cells |
| Body-strong | 13 / 600 / 1.45 | row titles, emphasized labels |
| Title-3 | 15 / 600 / 1.35 | section headers, dialog titles |
| Title-2 | 18 / 600 / 1.3 | pane headers |
| Title-1 | 24 / 700 / 1.25 | empty states, onboarding |
| Mono-data | 12–13 / 400 / 1.4 | sizes, ratios, hashes — tabular figures |

Arabic renders one size step up (+1px) at Body and below where the toolkit allows, to compensate for smaller apparent x-height.

## 4. Iconography & imagery

**Icons: one custom outlined SVG set — Lucide (ISC license) as the base, 24×24 grid, 1.5px stroke, rounded caps/joins — rendered per platform conventions.** Not per-platform native icon fonts: SF Symbols / Fluent Icons can't be shared across three toolkits, and a single stroke set keeps GUI–docs–website consistent. Sizes: 16 (rows, inline), 20 (toolbar), 24 (empty states). File-type icons come from the OS association APIs where available; Squash-drawn archive glyph as fallback. No emoji as icons, ever. RTL: directional icons (arrows, back/forward, progress chevrons) must flip; symmetric and file icons do not.

**Brand mark brief (for `art-asset-designer`):**
- Concept: a **box/crate being gently pressed** — a cube with two opposing chevrons or a vice-like frame compressing it; reads at 16px as a package, at 512px as "compression."
- Metaphor: press/squeeze, not destruction — Squash makes things *smaller and neat*, not broken.
- Form: flat, single-color-capable silhouette first (must work as monochrome macOS menu-bar/Windows tray icon), then one-accent version using `#2563EB`. Squircle/rounded-square container for app icon; no text in the mark.
- Avoid: literal fruit/vegetable (the other "squash"), trash-compactor/crushing imagery, 7-Zip-style numeral mimicry, gradient-heavy 2000s gloss, thin detail that dies below 32px.

## 5. Motion

Principles (all durations respect `prefers-reduced-motion` → instant state swap):
- **150–250ms, ease-out** for all micro-interactions; nothing longer than 400ms anywhere in the app.
- **Progress is data, not decoration:** bars animate via value interpolation, throttled to ~10 updates/sec; indeterminate shimmer only while the engine computes totals, switching to determinate the moment bytes are known.
- **Drag-and-drop:** drag-over highlights the drop zone (accent border + tinted surface) within 100ms; accepted drop plays a 200ms settle (row fades/slides into queue). Invalid payload shows the error-colored no-drop cursor immediately — no silent rejection.
- **Queue state changes:** rows reorder with layout (FLIP-style) animation ≤250ms; completion draws a check + success tint for 400ms then settles; failures shake once (≤200ms) and pin an inline error — never a toast that disappears.
- Exit faster than enter; never animate width/height of large surfaces.

## 6. Component tokens

- **Spacing (4pt grid):** `4 / 8 / 12 / 16 / 20 / 24 / 32 / 48`. Defaults: control padding 8×12, row padding 8×16, section gaps 24, dialog padding 24.
- **Corner radii:** `2` badges · `4` inputs/small buttons · `6` buttons/cards · `8` dialogs/panels · `999` pills/progress bars. Nothing above 8 except pills — utilities have corners.
- **Elevation:** flat — no shadows on resting surfaces; separation via 1px `Border` token. Only floating layers get shadow: menus/popovers `0 4px 16px rgba(15,23,42,.12)` (light) / `rgba(0,0,0,.5)` (dark); dialogs `0 8px 32px` same hues.
- **Control heights:** 28 compact (toolbar), 32 default (buttons/inputs), 40 primary CTA. Min hit target 28×28 desktop; keyboard focus ring = 2px `Primary` with 2px offset, never removed.
- **Progress bar:** height 6, radius 999, track = `Border` color, fill = state color from §2.
- **Queue row:** height 44, 16px icon, mono metadata column right-aligned (tabular figures), state badge at trailing edge (leading edge in RTL).
- **Borders in RTL:** progress fills and directional padding mirror automatically; mono/path columns do not mirror.

## Decisions locked

Native-flat style, blue `#2563EB` accent, system font stacks + Noto Sans Arabic + JetBrains Mono, Lucide icon set, 4pt/≤8-radius/no-resting-shadow tokens. Deviations require editing this doc first.
