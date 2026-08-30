---
name: terminal-adaptive
description: "Build developer-tool landing pages that read as software in light, dark, or system-adaptive themes: real compiling code as the hero, terminal motifs, hairline borders, syntax-led colour, and full theme parity. Use for CLI, API, infrastructure, database, open-source, and other developer-product landing pages, especially when the brief asks for light mode, dark mode, a theme toggle, system preference, code in the hero, or a page that feels like the product rather than generic SaaS marketing."
---

# Terminal Adaptive

A landing-page system for developer tools in **light, dark, or system-adaptive**
themes. The page should feel like the product it sells: precise, technical, dense,
and built around real code. Light mode is crisp and editorial; dark mode feels like
a serious editor or terminal. Both are first-class designs.

The failure mode is a generic SaaS template painted black, or a dark page inverted
to white. Theme adaptation is not colour inversion. Each theme needs its own canvas,
surface ladder, borders, text tiers, syntax colours, states, and elevation cues.

Before building, decide the theme contract:

- `light`: ship the light palette only.
- `dark`: ship the dark palette only.
- `system` or `dual`: implement both, default to `prefers-color-scheme`, offer an
  accessible toggle when appropriate, and persist an explicit user choice.

If the brief does not choose, use `system`. In `dual` mode, both themes must reach
the same quality bar before the work is complete.

## 1. Core idea

> Show the code. Respect the reader. Derive each theme.

Three rules drive the system:

1. **Real code is the hero visual.** Show the command or API the developer actually
   uses. It must compile or be valid for its language and demonstrate the product.
2. **Themes share semantics, not raw colours.** Components consume tokens such as
   `bg-base`, `text-secondary`, and `syntax-string`. Light and dark provide separate
   values for those tokens.
3. **Density is respect.** Use concrete information, compact rhythm, and credible
   product evidence. Do not pad three words and a button into `90vh`.

The two themes should feel related, not identical:

- **Light:** paper-like canvas, white technical surfaces, cool-grey rules, dark ink,
  restrained blue interaction colour, and syntax chosen for white backgrounds.
- **Dark:** near-black canvas, stepped charcoal surfaces, luminous-but-not-neon text,
  quiet blue interaction colour, and syntax chosen for charcoal backgrounds.

Do not change information architecture merely because the theme changes. A theme
toggle must not move the hero from centred to split, reorder content, or cause layout
shift. The reference mood may use either a centred editorial hero or a split
copy-and-code hero; choose based on content and viewport, then keep that composition
stable across themes.

## 2. Page architecture

Canonical order; adapt the middle to the actual product:

| # | Section | Purpose |
|---|---|---|
| 1 | Nav | Wordmark, docs/pricing/GitHub, star count, one action; `56–64px` with a hairline bottom rule. |
| 2 | Hero | One precise claim, real code or terminal, install command, and at most two actions. |
| 3 | Quickstart | The shortest copy-paste path to a working result. |
| 4 | How it works | Two or three real code examples showing the actual API. |
| 5 | Capabilities | Dense evidence: code, config, output, constraints, or formats. |
| 6 | Terminal / diff | A credible session or before/after diff. |
| 7 | Performance | Reproducible numbers in a table or compact benchmark strip. |
| 8 | Proof | Real stars, users, maintainers, testimonials, repos, or citations. |
| 9 | Pricing / OSS | Tiers or license and sponsor information. |
| 10 | CTA | Repeat the install command and docs link. |
| 11 | Footer | Dense sitemap plus version, license, and commit metadata. |

If a section could appear unchanged on a project-management SaaS page, replace it
with something product-specific.

### 2.1 Composition and rhythm

- Vary section shapes. No two adjacent sections should use the same composition.
- Prefer one strong product surface over three equal marketing cards.
- Do not repeat a centred eyebrow + heading + paragraph stack for every section.
- Section headings usually sit on the grid: headline left, explanation right, with a
  small monospace index such as `2.0 Migrating →`.
- A centred hero is valid when the core demo is a single wide code/terminal surface.
- A split hero is valid when copy and code need equal visual weight. At desktop use
  roughly `5/7` or `6/6`; collapse to one column on mobile.

### 2.2 Two-tone headline

The hero and major headings may be one sentence in two text tiers. The first clause
uses `text-primary`; the continuation uses `text-secondary`. Keep it in one semantic
heading so it wraps as one shape.

```html
<h1 class="hero-title">
  Your job queue is
  <span class="text-secondary">a Postgres table.</span>
</h1>
```

No gradient text. Size and tonal contrast do the work.

## 3. Theme system

### 3.1 Semantic tokens

Use semantic tokens everywhere. Never scatter raw theme colours across components.

```css
:root {
  color-scheme: light;

  --bg-base: #fafafa;
  --bg-surface: #ffffff;
  --bg-elevated: #f3f4f6;
  --bg-subtle: #f7f7f9;

  --border-subtle: #dedfe3;
  --border-strong: #c7cad1;

  --text-primary: #17181b;
  --text-secondary: #666970;
  --text-tertiary: #767b84;
  --text-disabled: #a4a8b0;

  --accent: #0878d1;
  --accent-hover: #0668b6;
  --accent-soft: #e9f4ff;
  --focus-ring: #0878d1;

  --syntax-keyword: #7c3aed;
  --syntax-string: #137547;
  --syntax-function: #0868b8;
  --syntax-number: #a94721;
  --syntax-comment: #68707c;
  --syntax-constant: #805900;
  --syntax-error: #bd3039;

  --button-primary-bg: #0878d1;
  --button-primary-fg: #ffffff;
  --button-primary-hover: #0668b6;
}

[data-theme="dark"] {
  color-scheme: dark;

  --bg-base: #090b0d;
  --bg-surface: #101216;
  --bg-elevated: #16181d;
  --bg-subtle: #0d0f12;

  --border-subtle: #242830;
  --border-strong: #333842;

  --text-primary: #e7e9ed;
  --text-secondary: #9ba1ac;
  --text-tertiary: #7d8590;
  --text-disabled: #505761;

  --accent: #82aaff;
  --accent-hover: #a8c2ff;
  --accent-soft: #172238;
  --focus-ring: #82aaff;

  --syntax-keyword: #c792ea;
  --syntax-string: #c3e88d;
  --syntax-function: #82aaff;
  --syntax-number: #f78c6c;
  --syntax-comment: #7d8590;
  --syntax-constant: #ffcb6b;
  --syntax-error: #f07178;

  --button-primary-bg: #e7e9ed;
  --button-primary-fg: #090b0d;
  --button-primary-hover: #ffffff;
}
```

These are starting values, not permission to skip contrast testing. If a product has
an established brand colour, map it to `--accent` only after verifying it in both
themes; do not add a second competing accent.

### 3.2 Theme application

For `system` or `dual`:

1. Read a persisted explicit choice: `light`, `dark`, or `system`.
2. If absent or `system`, resolve `prefers-color-scheme`.
3. Apply `data-theme` to `<html>` before first paint to avoid a flash of the wrong
   theme.
4. Set `color-scheme` so native controls and scrollbars match.
5. Listen for system changes only while preference is `system`.
6. Expose the current state through an accessible control with a clear label.

```js
const saved = localStorage.getItem("theme") || "system";
const systemDark = matchMedia("(prefers-color-scheme: dark)").matches;
const resolved = saved === "system" ? (systemDark ? "dark" : "light") : saved;
document.documentElement.dataset.theme = resolved;
```

Use an inline pre-hydration script or framework equivalent in the document head.
Do not hide theme flash with a long blank loading screen.

### 3.3 Hard colour rules

- Never use `#000000` or `#ffffff` as a full-page canvas.
- Do not mechanically invert RGB values or apply `filter: invert()`.
- Never place light-theme syntax colours on dark code or dark-theme syntax colours
  on white code.
- One accent leads links, active tabs, focus rings, and selected data. Other syntax
  hues belong to real code and meaningful states.
- No purple-to-cyan gradients, aurora blobs, neon glows, or decorative rainbow.
- A subtle `mask-image` fade may reveal overflow; it is not a painted background.

### 3.4 Buttons and links

Primary CTAs intentionally differ by theme:

- Light: solid `accent` background with white text, matching the crisp blue action in
  the reference.
- Dark: light neutral button using `text-primary` over `bg-base`; keep blue available
  for links, focus, and code.
- Secondary action: text link or hairline button, never a second solid CTA.

All states—default, hover, active, focus, disabled—must exist in both themes.

## 4. Surfaces, borders, and elevation

### Light

- The canvas is slightly off-white; technical frames are white.
- Hairline borders do most separation. A restrained shadow is allowed only where a
  floating layer genuinely overlaps content, such as a popover.
- Avoid a page made from pale-grey rounded cards. Prefer rules, whitespace, and one
  large technical surface.

### Dark

- Depth comes from `bg-base → bg-surface → bg-elevated`, hairlines, and negative
  space—not drop shadows.
- Raised elements are slightly lighter than their parent.
- An accent ring is a focus cue, not decoration.

### Shared frame motif

Use at most one recurring structural motif: a double frame around things a developer
looks into—editor, terminal, diff, or benchmark—not around prose.

```html
<div class="product-frame">
  <div class="product-surface"><!-- code / terminal / table --></div>
</div>
```

```css
.product-frame {
  padding: 4px;
  border: 1px solid var(--border-subtle);
  border-radius: 16px;
  background: var(--bg-subtle);
}
.product-surface {
  overflow: hidden;
  border: 1px solid var(--border-subtle);
  border-radius: 12px; /* outer radius minus gap */
  background: var(--bg-surface);
}
```

Radii must be concentric: `inner radius = outer radius − gap`.

## 5. Typography

Use exactly two families:

- Sans UI face: Inter, Geist, or IBM Plex Sans.
- Mono code face: JetBrains Mono, Geist Mono, IBM Plex Mono, or Berkeley Mono.

| Token | Desktop | Mobile | Weight | Leading | Use |
|---|---|---|---|---|---|
| `h1` | `clamp(40px, 5vw, 68px)` | `36px` | 600 max | `1.03–1.08` | Hero claim |
| `h2` | `clamp(28px, 3vw, 40px)` | `26px` | 600 max | `1.15` | Sections |
| `h3` | `20px` | `18px` | 500 | `1.3` | Capability titles |
| `body` | `16–18px` | `15–16px` | 400 | `1.55–1.65` | Prose |
| `small` | `14px` | `14px` | 400 | `1.5` | Captions |
| `code` | `14px` | `13px` | 400 | `1.6` | Code and terminal |
| `code-sm` | `13px` | `12px` | 400 | `1.5` | Metadata and commands |

Monospace marks text the reader could type or the machine produced. Never use it for
headlines, body paragraphs, nav links, or buttons. Cap prose near `70ch`.

## 6. Code as the hero

The code surface is the principal visual. It must remain convincing in both themes.

- Use real selectable `<pre><code>`, never a screenshot of code.
- Show `8–16` meaningful lines in a hero, or the shortest complete example the API
  honestly permits.
- The example must compile or validate and use the real public API.
- Use build-time tokenisation (for example Shiki), with a separate light and dark
  theme mapped to the semantic syntax tokens.
- Include a filename/language bar and a working copy button in the top-right.
- Line numbers are optional; if present, make them `user-select: none` so copied code
  stays clean.
- Preserve whitespace and horizontal meaning; never soft-wrap code to fit mobile.
- One highlighted line may use `accent-soft` plus a `2px` accent rule.
- Reserve the rendered height so theme or tab changes do not cause CLS.

```html
<figure class="code-frame">
  <figcaption>
    <code>queue.ts</code>
    <button aria-label="Copy code">Copy</button>
  </figcaption>
  <pre tabindex="0"><code class="language-ts">…real code…</code></pre>
</figure>
```

The light code surface should resemble a clean document or editor; the dark surface
should resemble a serious IDE. Do not put a dark code box inside a light page by
default: that is not light-mode parity, only a dark component pasted into it.

## 7. Terminal and install command

- Use `$` for shell, `>` for a REPL, or a credible `user@host` prompt.
- Typed input uses `text-primary`; output uses `text-secondary`; success/error use
  semantic status colour plus a word or icon.
- Never colour the entire terminal green.
- Commands and output must be plausible for the actual tool.
- A one-time type-on is optional; an infinite typing loop is forbidden.

The install command is one line and immediately copyable. Package-manager tabs may
switch `npm / pnpm / yarn / bun` without changing block height. The prompt glyph must
not be copied.

## 8. Product evidence

- Feature claims need evidence: code, config, terminal output, schema, or a measured
  constraint.
- Prefer definition lists and columns separated by vertical rules over repetitive
  rounded cards.
- Put benchmarks in a table or compact rule-separated strip, with environment,
  payload, concurrency, version, and reproduction link.
- Use real logos, repos, people, and metrics only. Do not invent social proof.
- Draw technical diagrams with labelled lines and restrained fills. No 3D browser
  mockups, floating cubes, or fabricated dashboards.

If visualising quantities beside monospace data, compute width from the real value
and draw a `1px` rule in a `ch`-wide track. Do not fake proportions or use chunky
block glyphs such as `█▉▊`.

## 9. Motion

Motion is functional and theme-independent:

| Interaction | Duration | Easing |
|---|---|---|
| Enter viewport | `400ms` | `cubic-bezier(0.16, 1, 0.3, 1)` |
| Copy feedback | `150ms` | `linear` |
| Tab cross-fade | `150ms` | `ease-out` |
| Theme colour transition | `0–180ms` | `linear` |

- Animate only `opacity` and `transform` for entrances.
- Theme transitions must never animate on initial load and must not leave unreadable
  intermediate colours. Disable them for `prefers-reduced-motion`.
- Copy changes instantly to a check and announces success through `aria-live`.
- Nothing loops, pulses, shimmers, counts up, parallax-scrolls, or follows the cursor.

## 10. Responsive behaviour

- At desktop, use a centred wide hero or split hero according to content—not theme.
- Below `md`, collapse to one column with copy before code unless task completion
  clearly benefits from code first.
- Code uses `overflow-x: auto`, retains indentation, and never soft-wraps.
- Add a subtle edge mask to indicate horizontal overflow; remove it when scrolled to
  the end.
- Mobile code is `13px` minimum; compact long examples to the essential `6–8` lines
  with a “show full” control when necessary.
- Touch targets are at least `44×44px`.
- Test `375, 640, 768, 1024, 1280, 1536` in both themes.

## 11. Accessibility and theme parity

- Normal text and syntax tokens must meet WCAG `4.5:1`; large text must meet `3:1`.
- UI boundaries and focus indicators must remain perceptible in both themes.
- Tertiary text is not permission to ship unreadable `12px` metadata.
- Do not convey status through colour alone.
- The theme control needs an accessible name, visible focus, keyboard operation, and
  an honest state such as “Theme: system”.
- Respect `prefers-color-scheme`, `prefers-reduced-motion`, forced colours, and page
  zoom.
- Use real semantic elements. Code is `<pre><code>`, tables are `<table>`, nav is
  `<nav>`, and copy status uses `aria-live="polite"`.
- Verify browser chrome, form controls, selection colours, scrollbars, and SVG icons
  in both themes. SVGs should normally use `currentColor`.

## 12. Performance

- Target `LCP < 2.0s`, `CLS < 0.05`, and landing-route JS under `150KB` where the
  project allows.
- Highlight code at build time and emit both theme token sets without shipping two
  copies of the source markup.
- Subset one sans and one mono family; use metric-compatible fallbacks and
  `font-display: swap`.
- Reserve code/tab heights. Theme switching must produce zero layout shift.
- Keep the pre-hydration theme resolver tiny and dependency-free.
- Lazy-load below-the-fold demos and media.

## 13. Anti-patterns

Any one of these breaks the style:

1. A light design with `dark:` utilities bolted on after completion.
2. A dark design run through automatic inversion to create light mode.
3. Pure-black canvas or a glaring pure-white full-page canvas.
4. Dark code surfaces pasted unchanged into every light theme.
5. Theme switching that changes layout, content order, dimensions, or scroll position.
6. A flash of the wrong theme during initial paint.
7. A purple-cyan gradient, aurora mesh, neon glow, or decorative rainbow.
8. Shadow-based elevation on the dark canvas.
9. Pale rounded cards filling the light canvas.
10. Fake or non-compiling hero code.
11. Code rendered as an image or hand-coloured token by token.
12. Code without a copy action, or line numbers copied with it.
13. All-green Hollywood terminal output or a looping typewriter.
14. Monospace headlines, prose, nav, or buttons.
15. `font-extrabold`, three font families, or tight code leading.
16. Marketing claims without technical evidence.
17. Repetitive three-by-two card grids and identical section headers.
18. Fake dashboards, tilted browser frames, floating cubes, or invented proof.
19. Benchmark stat cards without test context or reproducibility.
20. Motion that loops, shimmers, pulses, counts up, or ignores reduced motion.

## 14. Implementation sequence

1. **Inspect the real product.** Identify the actual install command, shortest valid
   usage, output, claims, and measurable proof. Completion: every above-the-fold
   technical string is grounded in product truth.
2. **Choose the theme contract and composition.** Declare `light`, `dark`, or
   `system/dual`; choose centred or split hero from content. Completion: the contract
   and section shapes are explicit, and theme does not control layout.
3. **Build semantic tokens first.** Implement every colour/state token in each
   required theme. Completion: no component needs a theme-specific raw colour.
4. **Build the real code/terminal surface.** Tokenise, copy, scroll, and verify the
   example. Completion: the sample compiles or validates and works at `375px`.
5. **Build the page with evidence.** Vary section rhythm and replace generic claims
   with code, config, output, or measured facts. Completion: no section could be
   transplanted unchanged to generic SaaS.
6. **Verify every required theme.** Run the checklist below at all target widths.
   Completion: there are zero known parity, accessibility, responsive, or theme-flash
   failures.

## 15. Self-verification

Do not report completion until every applicable item passes.

**Truth and code**

- [ ] Hero code is real, valid, concise, selectable, and copyable.
- [ ] Syntax tokenisation is accurate in both light and dark themes.
- [ ] Commands, output, benchmarks, versions, and proof are not fabricated.

**Theme system**

- [ ] Required modes are explicit: `light`, `dark`, or `system/dual`.
- [ ] Light and dark use the same semantic tokens with separately derived values.
- [ ] Both themes cover surfaces, text, borders, syntax, actions, status, focus,
      selection, native controls, and SVGs.
- [ ] System preference and saved preference resolve before first paint.
- [ ] Switching theme causes zero layout shift and no scroll jump.
- [ ] No automatic colour inversion, pure-black canvas, or full-page pure white.

**Composition**

- [ ] Hero composition comes from content and stays stable across themes.
- [ ] Adjacent sections use different shapes; no shape repeats three times.
- [ ] At least one headline uses a restrained two-tone text hierarchy.
- [ ] Technical frames share one concentric radius system.
- [ ] Feature and benchmark claims carry evidence.

**Typography and interaction**

- [ ] Human-written text is sans; machine-adjacent text is mono.
- [ ] Exactly two font families; code leading is `1.6`.
- [ ] Copy, tabs, theme control, hover, focus, disabled, and reduced-motion states work.
- [ ] Nothing loops, glows, shimmers, counts up, or uses decorative parallax.

**Responsive, accessibility, performance**

- [ ] At `375px`, code scrolls horizontally without wrapping or page overflow.
- [ ] Both themes pass contrast checks for every text and syntax token.
- [ ] Keyboard, focus, zoom, reduced motion, and screen-reader labels are verified.
- [ ] No theme flash, hydration mismatch, or layout shift is visible.
- [ ] Code is highlighted at build time and the route stays within its performance
      budget.

**Developer smell test**

- [ ] Would an engineer trust the product after one screen? If the code is fake, the
      prose is vague, or either theme feels secondary, the answer is no—fix it.
