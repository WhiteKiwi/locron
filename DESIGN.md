# Locron Brand Guide

Locron is calm local control that explains itself. This guide is the durable source of truth for
product, documentation, and community-facing design. It defines a recognizable system without
locking future surfaces to the dashboard's layout.

## Promise and attributes

Locron makes local automation dependable without infrastructure ceremony. Four attributes govern
every design choice:

- **Warm:** approachable enough to invite exploration, never cute at the expense of truth.
- **Precise:** schedules, states, timestamps, and consequences are explicit.
- **Capable:** the product feels steady under real operational work.
- **Reassuring:** errors explain what happened and what the user can safely do next.

The product line is **“Cron that explains itself.”** Use it when the context needs Locron's benefit
in one sentence; do not turn it into decorative copy on every screen.

## Voice and tone

Write developer-to-developer in short, active sentences. Name the durable fact first, then its
effect, then the next safe action. Use sentence case for headings, controls, and status labels.

Warm microcopy belongs in entry, onboarding, success, and truly empty states. Errors, security,
cancellation, quarantine, interrupted work, and destructive confirmation are neutral and factual.
Never joke while work is failing or imply completion before a durable operation succeeds.

Do: “The daemon is not running. New runs stay queued until it starts.”

Don't: “Oops! Roki fell asleep again.”

## Name, wordmark, and Roki

`Locron` is the primary product identifier. Use the capitalized form in prose and the lowercase
`locron` form for commands, packages, paths, and the compact product mark. Give the mark clear
space of at least the height of its `o`; do not stretch, outline, rotate, recolor individual
letters, or place it on a noisy field.

Roki, the friendly robot in `assets/banner.jpg`, supports the wordmark and never replaces it. Use
Roki at high-empathy moments such as first entry, onboarding, or a truly empty product state. Keep
the original proportions and yellow eyes. Do not repeat Roki in dense tables, use mascot dialogue
for errors, or place playful character art beside a destructive action.

The small hand-drawn spark is the preferred low-cost signature inside functional UI. It is always
decorative and must be hidden from assistive technology.

## Color

Color roles and values below match the dashboard CSS tokens exactly. Light surfaces are primary;
the dark console is a focused technical counter-surface.

| Role / CSS token | Value | Use |
|---|---:|---|
| Canvas `--color-canvas` | `#F6F0E3` | Warm application background |
| Surface `--color-surface` | `#FFFCF6` | Cards, forms, tables |
| Raised `--color-raised` | `#FFFFFF` | Menus and elevated controls |
| Ink `--color-ink` | `#24231F` | Primary text, strong actions |
| Graphite `--color-graphite` | `#5F5B52` | Secondary text |
| Border `--color-border` | `#D8D0C1` | Functional boundaries |
| Accent `--color-accent` | `#F5C842` | Recognition, selected detail, focus signature |
| Link `--color-link` | `#355B88` | Text links |
| Success `--color-success` | `#246B45` | Successful/healthy text |
| Success surface `--color-success-soft` | `#E7F2EA` | Successful/healthy background |
| Danger `--color-danger` | `#A83B35` | Failure and destructive text |
| Danger surface `--color-danger-soft` | `#F8E8E5` | Failure background |
| Running `--color-running` | `#285D8F` | Active work text |
| Running surface `--color-running-soft` | `#E7EFF7` | Active work background |
| Caution `--color-caution` | `#875B12` | Warning text; distinct from brand yellow |
| Caution surface `--color-caution-soft` | `#F6EEDC` | Warning background |
| Unknown `--color-unknown` | `#665F54` | Disabled/unknown text |
| Unknown surface `--color-unknown-soft` | `#EFEBE3` | Disabled/unknown background |
| Console `--color-console` | `#171713` | Logs and terminal output |
| Console ink `--color-console-ink` | `#F4F0E7` | Console text |

The WCAG contrast target is at least 4.5:1 for normal text and 3:1 for large text, icons, focus,
and component boundaries. Calculated sRGB contrast evidence for the approved pairs:

| Foreground / background | Ratio |
|---|---:|
| Ink / canvas | 13.85:1 |
| Ink / surface | 15.36:1 |
| Graphite / canvas | 5.96:1 |
| Graphite / surface | 6.60:1 |
| Ink / accent | 9.90:1 |
| Link / surface | 6.83:1 |
| Success / success surface | 5.60:1 |
| Danger / danger surface | 5.29:1 |
| Running / running surface | 5.93:1 |
| Caution / caution surface | 5.14:1 |
| Unknown / unknown surface | 5.30:1 |
| Console ink / console | 15.80:1 |
| Accent / console | 11.32:1 |

Yellow is brand recognition, not warning. Every state keeps a text label or icon in addition to
color. Never place white text on yellow.

## Typography

Use the local system sans-serif stack for operational reading and the local system monospace stack
for commands, schedules, IDs, timestamps, bytes, and logs. No product surface depends on an
external font or CDN.

- Display: 2rem/1.05, 750 weight; wordmark and rare expressive empty states.
- Page title: clamp(1.75rem, 4vw, 2.6rem)/1.08, 750 weight.
- Section title: 1.125rem/1.3, 700 weight.
- Body: 1rem/1.6, 400 weight.
- Compact data: 0.875rem/1.45, 500 weight.
- Label: 0.75rem/1.3, 700 weight, modest tracking.

Use tabular numerals for comparable time and count columns. Avoid all-caps prose; short table
headers may use uppercase styling. Never use decorative handwriting for essential information.

## Illustration and icons

Illustration is editorial, not structural. The banner's paper texture, yellow strokes, books, cat,
laptop, and Roki may inspire onboarding or documentation, but do not become repeated card chrome.
Icons use a single outline weight, rounded joins, and a 20–24 px optical box. Pair unfamiliar icons
with text. Do not mix filled emoji, multiple icon families, or icon-only destructive actions.

## Layout and tokens

The shell uses large calm outer margins and compact internal data rhythm. The first scan should
answer: what is running, what happens next, what needs attention, and what action is safe.

- Spacing `--space-1` through `--space-8`: `0.25rem`, `0.5rem`, `0.75rem`, `1rem`, `1.5rem`,
  `2rem`, `3rem`, `4rem`.
- Radii `--radius-sm`, `--radius-md`, `--radius-lg`, `--radius-pill`: `0.5rem`, `0.75rem`,
  `1.125rem`, `999px`.
- Border `--border-ui`: `1px solid #D8D0C1`.
- Elevation `--shadow-low`: `0 1px 0 rgba(36,35,31,.06), 0 10px 30px rgba(36,35,31,.05)`.
- Content width `--content-max`: `72rem`.
- Motion `--motion-fast`, `--motion-base`: `120ms`, `180ms` with an ease-out curve.

Use one filled primary action per decision area. Secondary controls remain outlined or textual;
destructive controls recede until the destructive decision is present. Do not wrap every metric in
its own card.

## Components and states

- Buttons expose default, hover, active, focus, disabled, busy, and destructive states. A disabled
  button explains why with adjacent text or a title where appropriate.
- Inputs retain visible labels. Errors appear next to the field or decision area and never rely on
  placeholder text. Mobile inputs stay at 16 px or larger.
- Tables use a quiet header, row dividers, wrapping long values, and horizontal reachability on
  narrow screens. Full IDs and URLs remain inspectable.
- Chips pair semantic color with plain text. Spinners supplement “running” or “loading”; they do
  not replace the label.
- Notices use success, caution, danger, or neutral roles. Brand yellow never doubles as caution.
- Empty states name why the area is empty and offer the primary next action when one exists.
- Error states preserve the failed context and suggest a safe retry. They contain no mascot joke.
- The log console is dark, monospace, searchable, keyboard reachable, and uses yellow only for the
  live/final signal. Output order and truncation markers remain operational facts.

## Motion

Motion is snappy-gentle: 120–180 ms opacity, transform, background, or border feedback that explains
interaction and state change. It never delays a durable operation. Avoid ambient loops, scroll
spectacle, cursor trails, parallax, 3D, bouncing failures, or celebratory motion for routine work.

Under `prefers-reduced-motion: reduce`, remove smooth scrolling and nonessential transition or
animation. Loading state must remain understandable without a spinning element.

## Accessibility and responsive behavior

Use a skip link, semantic landmarks, meaningful labels, current-page semantics, visible
`:focus-visible`, logical DOM order, and full keyboard operation. Touch-oriented layouts use at
least 44 px targets. Support narrow mobile-sized viewports, 200% zoom, text scaling, long content,
and reduced motion without hiding the primary operation. Use `aria-live` only for concise dynamic
updates; logs must not continuously overwhelm assistive technology.

## Do and don't

- Do let cream, charcoal, one yellow signature, disciplined radii, and quiet data surfaces make a
  page feel like Locron.
- Do reserve Roki and expressive marks for entry, onboarding, and true empty states.
- Do keep operational hierarchy, timestamps, outcomes, and safe actions more prominent than art.
- Don't use generic purple/pink AI gradients, glassmorphism, neumorphism, heavy blur, rainbow
  status palettes, or excessive shadows.
- Don't add 3D, GSAP, Lottie, continuous animation, external fonts, or network assets merely for
  style.
- Don't copy a reference brand or gallery layout. External references are quality checks; Locron's
  banner and product promise are the identity.
