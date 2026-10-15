# FS-rhei-viz-ux: Console-First Visualization UX

This spec defines the look and feel of every Rhei visualization surface — the
browser dashboard [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization) and the terminal TUI [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal) — as a
single visual language. It does not change *which* views exist or *what* data
they show; it constrains *how* they present it. The governing requirement is
that a visualization must feel like a calm console, not a web application:
opening it should cause no startle and no context switch. Bright, busy, and
animated surfaces make everyday agent work feel like operating infrastructure
instead of reading a plan [§GND-rhei-purpose.1](../grund.md#1-the-problem), and they fight the goal of
monitoring that is useful and pretty enough to read at a glance
[§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals).

The principle in one line: **keep the calm of the command line, add the
structure of a UI, and add nothing else.**

## Goals

1. **No startle on open.** First paint is unremarkable. A user who runs `rhei
   run` and opens the printed dashboard URL should feel the terminal simply
   continued into a larger window, not that a new app launched.
2. **Minimal sensory strain.** Dark by default, desaturated chrome, a monospace
   grid, near-zero motion, and silence. The surface is restful to keep open for
   a long-running session.
3. **Color and motion carry meaning, never decoration.** Saturated color and any
   movement are reserved strictly for state and for things that need attention.
4. **One language across terminal and browser.** The dashboard is the TUI's
   larger sibling: same vocabulary, same state colors, same glyphs, same journal
   lines. Recognition transfers in both directions.
5. **Add the cool of a UI, quietly.** Layout, alignment, density, navigation,
   and charts are the value a UI adds over scrollback — delivered without noise.
6. **Investigate with the fewest clicks and least mental effort.** The dashboard
   exists to answer "what is my Rhei doing, and why," and to make that answer
   cheap to reach. The resting view shows the whole plan's shape and marks each
   node's state — what is done, live, blocked, gated, or idle — without a click.
   Any node, not only the running ones, can be entered to see its surroundings:
   what it depends on, what it unblocks, where it sits in its state machine and
   what it can move to next. Walking from a node to a neighbor and back costs a
   keystroke, never a tab hunt or a rebuilt mental model [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals).

## Non-Goals

- Not a redefinition of dashboard tabs, charts, or TUI tiles — that is
  [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization) and [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal). This spec is purely presentational.
- No marketing aesthetic: no hero sections, brand logos, illustrations,
  decorative gradients, glow, or drop shadows for drama.
- No audio, no browser notifications, no moving toasts.
- No general theming/skinning engine. Adaptive dark/light and reduced-motion
  only; palette presets are Future Work.
- No external assets. The self-containment rule of [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization) stands and is a
  strain and load-time benefit, not just a security one.

## 1. Design doctrine

Four stances resolve every later rule. When sections below leave a detail open,
decide it the way these stances point.

1. **The terminal is home; the browser is a window onto the same run.** The
   browser surface mirrors the TUI's framing — tiles, journal, legend, status
   strip — so the eye recognizes it instantly. A saved page should be
   indistinguishable from the live one except for liveness.
2. **Calm by default, loud only on meaning.** The resting state of every pixel
   is monochrome and still. Chroma and change are spent only where they inform.
3. **Quiet entrance.** No splash, no loading spinner, no animated reveal. The
   first frame shows the last-good snapshot immediately, consistent with the
   reload-tolerance rule of [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization).
4. **Two renderers, one model.** Anything a reader can name — a state, an id, an
   arrow, a severity, a cost — looks and reads the same in both surfaces.

## 2. Typography and layout

### 2.1. Monospace everywhere

All text — headers, prose, table cells, ids, states, logs, and numbers — is set
in one monospace stack:

```css
font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas,
  "Liberation Mono", "Roboto Mono", monospace;
```

There is no proportional/sans body font. The single typeface is the strongest
"this is just a terminal" signal, it keeps every column aligned to the character
grid, and it removes the font-pairing decisions that make a surface read as a web
dashboard. The browser dashboard therefore drops its current sans-serif body and
gradient header text in favor of the monospace stack already used for its data.

### 2.2. The character grid

- Tables, tiles, and legends align to monospace character cells. Numeric columns
  (cost, tokens, durations, counts) use `font-variant-numeric: tabular-nums` and
  are right-aligned so digits line up vertically.
- Panels are framed with 1px hairlines rather than shadows, echoing the
  box-drawn panels of the TUI. The browser may use box-drawing characters where
  it reinforces the terminal feel; it must not depend on them for meaning.
- Layout is left-aligned and uses the full width like a terminal; no centered
  narrow reading column.

### 2.3. A small type scale

Two weights only: `400` for body, `600` for emphasis (headings, active tab,
state labels). No italics for emphasis. A short fixed size ramp (roughly 11–17px
in the browser) and a comfortable line height (~1.45) keep dense data scannable
without bolding or coloring for hierarchy.

## 3. Color

### 3.1. Palette tokens (dark default)

Chrome is grayscale plus a single restrained accent. The dashboard exposes these
as CSS custom properties; the TUI maps the same roles to terminal colors. Values
are the normative direction and may be nudged to meet the contrast targets in
[§FS-rhei-viz-ux.3.3](rhei-viz-ux.spec.md#33-adaptive-theme-and-contrast).

```css
--bg:        #0e1116;  /* base surface, near-black neutral (not pure black) */
--surface:   #14181f;  /* raised panel */
--surface-2: #1b212b;  /* nested / header strip */
--hairline:  #232a35;  /* 1px borders and rules */
--ink:       #d6dae0;  /* primary text — off-white, not #fff, to cut halation */
--dim:       #9aa3af;  /* secondary text */
--faint:     #6b7480;  /* tertiary / disabled */
--accent:    #7fb0d0;  /* links, focus ring, active underline — used sparingly */
```

The base is neutral, not navy: a blue-tinted background reads as "app." Text is
deliberately off-white because pure white on near-black strains the eye over a
long session.

### 3.2. State color is shared, calm, and meaning-bearing

There is exactly one state-color map, consumed by both the dashboard and the TUI
so a `blocked` task is the same color in scrollback and in the browser. Two rules
shape it:

- **Chrome stays monochrome.** Saturated hue appears only on state pills/cells,
  journal severity, and attention banners. Tabs, tables, panels, headers, and
  prose are grayscale plus `--accent`.
- **Chroma is proportional to attention.** States that demand action keep their
  hue; idle, terminal, and done states fade toward the chrome so the eye is
  drawn only to what needs it.

| State | Today | Calm (dark) | Chroma rationale |
| --- | --- | --- | --- |
| `draft` | `#64748b` | `#5b6573` | idle → near-gray |
| `pending` | `#94a3b8` | `#7c8694` | idle → dim neutral |
| `in_progress` / `in-progress` | `#3b82f6` | `#5a8fc7` | live → identifiable blue |
| `active` | `#38bdf8` | `#6bb0cf` | live → kept legible |
| `needs-review` | `#f59e0b` | `#c79a4e` | attention → retains amber |
| `human-review` | `#22c55e` | `#6cae7c` | gate → present green |
| `review` | `#a855f7` | `#9b7fc4` | working → muted violet |
| `prove` | `#06b6d4` | `#4fa3b3` | working → muted cyan |
| `consolidate` | `#14b8a6` | `#4fa394` | working → muted teal |
| `fix` | `#f97316` | `#c98552` | working → muted orange |
| `agent-review` | `#8b5cf6` | `#8a78c4` | working → muted indigo |
| `agent-review-fix` | `#ec4899` | `#c47596` | working → muted pink |
| `blocked` | `#ef4444` | `#cf5b5b` | attention → retains red |
| `failed` | `#ef4444` | `#cf5b5b` | attention → retains red |
| `completed` | `#10b981` | `#4f9e7e` | done → settled green |
| `cancelled` | `#475569` | `#424b57` | terminal → near-gray |
| `archived` | `#334155` | `#353c46` | terminal → near-gray |

Custom states keep the existing stable name-derived fallback from [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization),
but desaturated to the same level so a project state never out-shouts a built-in
attention state. Cost heatmaps ([§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)) use a single-hue ramp
of `--accent`, not a rainbow.

### 3.3. Adaptive theme and contrast

- **Light theme** follows `prefers-color-scheme: light`: surfaces invert to a
  warm off-white (not pure white), `--ink` becomes near-black, and the same
  state hues are darkened to hold contrast. Hue identity is preserved across
  themes.
- **Contrast** meets WCAG AA for text (≥ 4.5:1 body, ≥ 3:1 large/`600`). State
  pill text is checked against its own fill; pills auto-pick a dark or light
  label per fill luminance.
- **Color is never the only signal.** Every state pill carries its text label,
  and severity carries a glyph ([§FS-rhei-viz-ux.6](rhei-viz-ux.spec.md#6-continuity-with-the-terminal)), so the surface remains readable under
  `prefers-contrast: more` and for color-vision deficiency.

## 4. Motion and liveness

Near-zero motion is a hard requirement, not a preference.

- **Banned:** spinners, progress pulses, marquees, sliding/expanding panels,
  parallax, charts that grow or sweep on load, blinking, color cycling,
  skeleton-shimmer loaders, auto-scroll.
- **Allowed:** instant state application, and at most a single ≤150ms opacity
  fade on content swap. Under `prefers-reduced-motion: reduce`, even that fade is
  dropped — changes are instantaneous.
- **Liveness is shown by content changing in place, not by movement.** A polled
  refresh updates a row's text and pill where it sits. A just-changed row may
  receive one brief, low-contrast background tint that decays to nothing; this
  cue is suppressed under reduced motion.
- **No layout shift on poll.** Refreshing the snapshot must not reflow, resize,
  or scroll-jump. Ordering is stable, space is reserved for optional fields, and
  the user's scroll position and text selection survive a refresh — the surface
  is meant to be read while it updates.

## 5. Silence: sound and notifications

- No audio of any kind.
- No browser notifications and no popup/toast layer. Status changes surface only
  in the existing status strip, journal pane, and banner region.
- The banner (`error` / `done` states already present in the dashboard) is the
  single escalation channel; it appears in place and does not animate in.

## 6. Continuity with the terminal

The bridge between surfaces is concrete, not thematic:

- **Shared vocabulary and glyphs.** State names, task ids, and the transition
  arrow `→` (U+2192) render identically in both surfaces, with an ASCII `->`
  fallback where U+2192 is unavailable. Severity glyphs are a fixed set (e.g.
  `i` info, `!` warn, `x` error) shared by the TUI journal and the dashboard
  journal.
- **Identical journal lines.** The dashboard journal pane shows the same
  fixed-column lines as `runtime/transitions.log` [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal) — a reader
  can `tail -f` the file or watch the browser and see the same text.
- **Mirrored framing.** The dashboard's tiles, journal, legend, and status strip
  reuse the TUI's spatial arrangement so layout knowledge transfers.
- **Quiet entry and exit.** The run prints the dashboard URL as a plain link;
  opening it shows the cached last-good snapshot with no spinner. The frozen
  final HTML artifact written under `runtime/` uses this exact language, so an
  archived view is indistinguishable from a live one minus updates.

## 7. Density, focus, and disclosure

The "cool from the UI" is structure, delivered calmly.

- **Progressive disclosure.** The calmest overview is the default; dense detail
  lives behind the existing tabs and on-demand rows, never in modal pile-ups.
- **Keyboard-first, mouse-optional.** Like the CLI, the dashboard is fully
  navigable from the keyboard: tab switching by digit/`h`–`l`, `/` to filter, and
  a visible `--accent` focus ring. The mouse is supported but never required.
- **Quiet empty states.** Empty views render the useful placeholder of
  [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization) in monochrome — a short line of guidance, never a blank or a
  decorative illustration.
- **Selectable text.** All content is selectable and copy-friendly; it is a
  console, and copying an id, a path, or a journal line must just work.

## 8. Accessibility and resilience

- Honor `prefers-color-scheme`, `prefers-reduced-motion`, and `prefers-contrast`.
- Meet the contrast targets of [§FS-rhei-viz-ux.3.3](rhei-viz-ux.spec.md#33-adaptive-theme-and-contrast) and never encode meaning in color alone.
- Stay self-contained: no external fonts, scripts, styles, or network assets, so
  the surface loads instantly and works offline (restated from [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)).
- Degrade without drama: the TUI already collapses to a compact list on small
  terminals [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal); the browser stacks panels on narrow viewports
  rather than truncating or animating.

## 9. Anti-patterns: what "flashiness" means here

These are explicitly forbidden, in any surface, so reviewers have a shared bar:

- Decorative gradients, neon/glow, and shadows used for depth-drama.
- Animated number counters, charts that animate in, confetti, shimmer loaders.
- Spinners or progress bars for sub-second polls; blinking or pulsing anything.
- Toasts, modals, or notification trays that slide, stack, or auto-dismiss.
- Sound, haptics, or attention-seeking favicons/title flashes.
- SaaS-style stacked rounded cards, oversized hero headers, or brand logos.
- More than one accent color in the chrome, or saturated color on non-state UI.

## Related Specifications

- [Flow Visualization](rhei-viz.spec.md) — which views exist and what data they
  render.
- [`rhei run` TUI and Transition Journal](rhei-run-tui.spec.md) — the terminal
  surface, slot layout, and journal format this language shares.
- [Cost Accounting](rhei-cost-accounting.spec.md) — the accounting data the Cost
  view and cost heatmaps present.

## Future Work

- User-selectable terminal palette presets (e.g. solarized, gruvbox) layered on
  the adaptive default.
- A density toggle (comfortable vs. compact) for the browser tables.
- Optional high-contrast theme beyond `prefers-contrast` defaults.
