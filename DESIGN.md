# Locron Brand and Interface Guide

Locron is **calm local control that explains itself**. The identity source is `assets/banner.jpg`:
warm paper, charcoal clarity, sunny yellow recognition, Roki, and sparse hand-drawn signatures. The
operator interface translates that character into a modern, precise cockpit without copying another
product's composition or ornament.

## Character and voice

Write developer-to-developer in short active sentences: durable fact, effect, then next safe action.
Friendly language belongs in entry and genuinely empty states. Errors, security, cancellation,
quarantine, and destructive work stay neutral and factual. Never imply completion before a durable
operation succeeds.

`Locron` is the prose name; `locron` is the command and compact mark. Roki supports but never replaces
the wordmark. Preserve Roki's proportions and yellow eyes, and do not put mascot dialogue beside a
failure or destructive action. Decorative marks are sparse and hidden from assistive technology.

## Exact semantic schemes

Light and dark are authored peers. Components use semantic names, never literal scheme branches.
Amber is brand focus and selection, not warning. Every status retains a text label.

| CSS token | Light | Dark | Role |
|---|---:|---:|---|
| `--color-canvas` | `#F7F5EF` | `#151512` | application background |
| `--color-surface` | `#FCFBF7` | `#1C1C18` | opaque workbench |
| `--color-raised` | `#FFFFFF` | `#24231E` | menus and dialogs |
| `--color-hover` | `#F4F0E6` | `#25241F` | quiet interactive hover |
| `--color-pressed` | `#EBE5D7` | `#2D2B24` | pressed interaction |
| `--color-selected` | `#FFF0C2` | `#3A2C0D` | current/selected surface |
| `--color-border` | `#D9D5CA` | `#3A3931` | passive divider |
| `--color-border-control` | `#8D887E` | `#747164` | interactive boundary |
| `--color-text` | `#211F1A` | `#F3F0E8` | primary foreground |
| `--color-muted` | `#6A655B` | `#AAA69B` | secondary foreground |
| `--color-disabled-text` | `#817C72` | `#858176` | disabled foreground |
| `--color-accent` | `#E3A91D` | `#E4AD2B` | brand marker/selection |
| `--color-accent-text` | `#7A4A00` | `#F0BD4C` | amber-associated text |
| `--color-accent-soft` | `#FFF0C2` | `#3A2C0D` | selected background |
| `--color-on-accent` | `#241A00` | `#201800` | foreground on amber |
| `--color-focus` | `#B87500` | `#E4AD2B` | focus ring |
| `--color-primary` | `#211F1A` | `#F3F0E8` | primary button |
| `--color-on-primary` | `#FFFFFF` | `#151512` | primary button text |

Status foreground/background pairs are:

| CSS token | Light | Dark |
|---|---:|---:|
| `--color-success` | `#176B4C` | `#70D4A7` |
| `--color-success-soft` | `#E7F5EE` | `#193329` |
| `--color-warning` | `#795000` | `#F0BD4C` |
| `--color-warning-soft` | `#FFF3CC` | `#382B0D` |
| `--color-danger` | `#A73531` | `#F07872` |
| `--color-danger-soft` | `#FCECEA` | `#3D211F` |
| `--color-info` | `#245E8C` | `#83B9EB` |
| `--color-info-soft` | `#EAF3FC` | `#1D2E3D` |
| `--color-console` | `#171713` | `#0F0F0D` |
| `--color-console-text` | `#F3F0E8` | `#F3F0E8` |

Calculated sRGB WCAG ratios are light text/canvas 15.1:1, muted/canvas 5.31:1, and
on-accent/accent 8.14:1; dark text/canvas 16.06:1, muted/canvas 7.52:1, and
on-accent/accent 8.65:1. Control borders exceed 3:1 against their ordinary surface. Normal text
targets 4.5:1; large text, icons, focus, and component boundaries target 3:1.

## Typography and provenance

Operational sans uses locally embedded **Geist Sans Variable**; code, IDs, schedules, times, and
logs use **Geist Mono Variable**. Both are pinned to official `vercel/geist-font` tag `v1.7.2`,
release asset `https://github.com/vercel/geist-font/releases/download/v1.7.2/geist-font-v1.7.2.zip`,
under the unmodified `crates/locron-server/assets/fonts/OFL.txt`.

- `GeistSans-Variable.woff2` SHA-256:
  `a369fcf5628ea2aa4e1b9e2ec6a5b3624e365bda588e1f0f2f12b564f728fbb8`
- `GeistMono-Variable.woff2` SHA-256:
  `fba8f577f38a2bbcbe818efa6348dd58f36303a10b8737c42fefad275be563ab`
- Upstream `OFL.txt` SHA-256:
  `c683bfbcc7e087f5d37a54ef628f10387c451a83ddc459b151403a164ac46c90`.
  The repository copy has a final newline and SHA-256
  `2b2da563e79400b61818402ca9f26a73d52468268b7fc715e92143c1e799737e`.

Preload Sans only; both faces use `font-display: optional`. Sans fallbacks are `-apple-system`,
`BlinkMacSystemFont`, Apple SD Gothic Neo, Noto Sans KR, Malgun Gothic, system UI, and sans-serif;
do not assume an unbundled Pretendard installation. Mono falls through to `ui-monospace`,
SFMono-Regular, Menlo, Consolas, and monospace. Enable optical sizing and kerning, disable synthetic
faces, retain normal sans ligatures, and disable mono ligatures. Comparable numerals are tabular.

| Role | Size / line / weight / tracking | Mixed Korean/Latin tracking |
|---|---|---|
| empty/display | 32 / 40 / 650 / `-.025em` | `-.012em` |
| page title | 24 / 32 / 650 / `-.018em` | `-.010em` |
| section title | 18 / 26 / 620 / `-.012em` | `-.006em` |
| subsection | 15 / 22 / 600 / `-.006em` | `0` |
| nav/menu/control | 14 / 20 / 540 / `-.006em` | `0` |
| body/copy | 14 / 21 / 420 / `-.003em` | `0` |
| field label | 13 / 18 / 560 / `-.004em` | `0` |
| table primary | 13 / 18 / 500 / `-.003em` | `0` |
| metadata/caption | 12 / 17 / 450 / `+.005em` | `0` |
| JSON/code | 13 / 20 / 430 / `0` | `0` |
| mobile input | 16 / 22 / 420–500 / `0` | `0` |

Korean operational labels never receive all-caps or negative tracking. Nothing is below 12 px.

## Flat material, scale, and layout

The canvas is near-solid and the workbench is opaque. Thin dividers, generous outer space, and one
high-contrast primary action establish hierarchy. Never use ambient gradients, broad glow, texture,
glass workbenches, nested blur, or a card around every field/value. Only transient menus/tooltips use
localized material. Sticky scrolling chrome uses light `rgb(252 251 247 / .86)` or dark
`rgb(28 28 24 / .82)`, 14 px blur, and 108% saturation. Transient menu/tooltip material uses light
`rgb(255 255 255 / .92)` or dark `rgb(36 35 30 / .90)`, 16 px blur, and 110% saturation. Light
hairlines are inset `rgb(255 255 255 / .72)` plus outer `rgb(33 31 26 / .10)`; dark hairlines are
inset `rgb(255 255 255 / .10)` plus outer `rgb(255 255 255 / .12)`. Local shadows are light
`0 10px 28px rgb(33 31 26 / .10)` and dark `0 12px 32px rgb(0 0 0 / .32)`.

Always declare the opaque surface/raised fallback before an `@supports` enhancement. Rails,
workbench, tables, forms, JSON/code, notices, and dialogs remain opaque. Forced colors, increased
contrast, reduced transparency, and `[data-material="solid"]` remove blur and translucency. Modal
smoke is light `rgb(21 21 18 / .38)` or dark `rgb(0 0 0 / .58)`; dialog content stays opaque.

Spacing is 4, 8, 12, 16, 24, 32, 40, 48, and 64 px. Radii are 4 px for status, 6 px controls,
8 px menus/sections, and 12 px dialogs. Pills are limited to status and compact filters. Compact
controls are 36 px, form controls 40 px, multiline fields at least 96 px, desktop icon buttons
32 px, table headers 36 px, rows 44 px, and touch controls at least 44 px.

Form slots use 8 px from label or legend to control, 8 px from the final segmented choice edge to
its group help, 4 px between ordinary control/help/error slots, 20 px to the next field, and 40 px
between sections. Group help stays in normal flow and is programmatically associated with its
fieldset. Theme help is 13/18 muted text and no wider than 56 characters.

After a successful zero-result response, Jobs and Run history retain their toolbar, table frame, and
36 px header. One body cell spans every visible column and provides a centered 112 px content block
inside a 160 px row with 24 px padding; the narrow semantic-list equivalent is at least 96 px with
24 px by 16 px padding. Filtered zero offers Clear filters, first use offers its next job action,
pagination disappears, and only the adjacent result count—not the empty body—is a live region.

At 1024 px and wider, use an opaque 224 px rail. At 768–1023 px it becomes a 64 px icon rail with
labelled tooltips. Below 768 px, use a 56 px top bar and four equal labelled bottom destinations.
The route and daemon state stay visible. Desktop Jobs and Run history use comparison tables; below
760 px they use semantic object rows containing the same core facts and named actions. The job form
uses a bounded 720 px measure plus a 176 px sticky section rail and solid sticky action bar.

## Components, interaction, and access

Fixed enumerations use the Locron Select wrapper over pinned Radix Select. Commands use DropdownMenu,
short blocking decisions use Dialog, and icon-button labels may be supplemented by Tooltip. Radio
and checkbox semantics remain native. Date/time stays a native input inside an authored wrapper.
Popup layers portal into the application-owned sibling root. No custom ARIA listbox or full UI kit.

Labels, help, validation, consequences, and status are visible text. Focus uses a strong
`:focus-visible` ring. Preserve logical DOM order, skip navigation, `aria-current`, concise live
updates, non-live logs, 200% reflow, long-value wrapping, and both schemes. Motion is 80 ms press,
120 ms hover/focus, 160 ms popup/disclosure, and 200 ms dialog/state with opacity/transform only.
Reduced motion removes transforms and shortens opacity; reduced transparency uses opaque layers.

The exact-source JSON viewer lexes RFC 8259 strings/escapes, numbers, literals, punctuation, and
whitespace without parsing and reserializing. React text nodes render the untouched source inside
one continuous `<pre><code>` value; never inject HTML. Its opaque toolbar exposes JSON, exact Copy
with visible status, and a locally persisted Wrap toggle. Invalid input remains exact, copyable, and
labelled `Invalid JSON`. Sources over 200 lines or 64 KiB initially show 80 complete lines while
Copy retains the full source; expanded content scrolls within 480 px desktop or 360 px narrow.

Desktop and mobile Job/Run rows retain exactly one descriptive native detail link plus a separately
named menu. A shared pointer helper follows that link only for an unmodified primary click on blank
row surface with no text selection. It ignores prevented, modified, non-primary, interactive/menu
descendants, and selection. Rows never receive link roles, tabindex, keyboard handlers, overlay
anchors, lift, or shadow. Labelled 224 px and mobile navigation never mount duplicate tooltips;
tooltips are reserved for the actual icon-only 64 px rail and supplemental icon-only buttons.

Do retain cream/charcoal/yellow recognition, calm hierarchy, original Roki/wordmark/favicon, and a
dark bounded log console. Do not copy another brand, use neon AI styling, neumorphism, WebGL/3D,
CDN fonts, external assets, GSAP, Lottie, parallax, ambient loops, hover-only actions, tooltip-only
instructions, icon-only core actions, nested dialogs/submenus, or essential generated CSS copy.
