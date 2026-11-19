# UI tips

General UI tips the owner collected from two reference images (studies by large tech companies). They are **tips, not rules**: apply them where they help. The source images gave only the names; the one-line meanings and the "In TZW" notes are ours.

## 1. Niche design hacks

| # | Tip | Meaning | In TZW |
|---|---|---|---|
| 1 | Concentric radius | A nested element's corner radius = outer radius − padding, so curves stay parallel. | `--r-outer` / `--r-inner` tokens; cards (10px) holding controls (6px) with 4px inset. |
| 2 | Alpha borders | Borders as translucent white/black instead of a fixed grey, so they adapt to any surface. | `--line` = white at 8%, `--line-strong` at 14%. |
| 3 | OKLCH palettes | Define colours in OKLCH so steps are perceptually even and hues keep their lightness. | All tokens in `globals.css` are OKLCH; agent series colours share one L and C, only the hue changes. |
| 4 | OKLCH gradients | Interpolate gradients in OKLCH to avoid the grey "dead zone" of sRGB blends. | Used sparingly: chart area fills (`in oklch`). |
| 5 | Matched icon weight | Icon stroke weight matches the text weight beside it. | Custom icon set at 1.5px stroke next to 500-weight labels. |
| 6 | Masked edges | Fade scroll containers at their edges with `mask-image` rather than hard clipping. | Sidebar nav, horizontal tab rows, the block menu list. |
| 7 | Blurred icon swaps | When an icon changes, cross-fade with a short blur so it morphs instead of popping. | Mode switch icon (basic ↔ advanced). |
| 8 | Dark mode depth | In dark mode, elevation = lighter surface, not a bigger shadow. | `--bg` < `--bg-2` < `--surface` < `--surface-2`; shadows only on floating panels. |
| 9 | Dynamic viewport | Use `dvh`/`svh` units so full-height layouts survive mobile browser chrome. | Shell and logic canvas use `100dvh`. |
| 10 | Multiply blending | Blend overlays with multiply (or screen in dark mode) so they tint instead of cover. | Selection highlights on charts use `mix-blend-mode: screen`. |

## 2. UX and animation principles

| # | Tip | Meaning | In TZW |
|---|---|---|---|
| 1 | Squash & stretch | Objects deform slightly under motion to feel physical. | Buttons compress (scale .97) on press. |
| 2 | Anticipation wind-up | A small preparatory move before the main one. | Panels nudge before opening is skipped (too slow for tools); used only on press states. |
| 3 | Staging the focus | Direct attention to one thing at a time. | One dominant element per page: the P&L chart in Advanced, the portfolio value in Basic. |
| 4 | Pose to pose | Design key states first, then the transitions between them. | Interaction states (proposed → settled) are designed as explicit badges first. |
| 5 | Follow through | Motion settles past its target slightly before stopping. | Side panel opens with a soft overshoot easing. |
| 6 | Slow in & slow out | Ease in and out; nothing moves at constant speed. | `--ease-out` for entering, `--ease-in` for leaving. |
| 7 | Moving in arcs | Natural motion follows curves, not straight lines. | Canvas connection lines are curved (bezier). |
| 8 | Secondary action | Small supporting motion that reinforces the main one. | Chart crosshair dot follows the cursor while the tooltip updates. |
| 9 | Snappy timing | Keep UI transitions short (≈120–250 ms). | `--dur-1` 120ms, `--dur-2` 200ms, `--dur-3` 280ms. |
| 10 | Exaggerated feedback | Make results of actions unmistakable. | Saved/approved toasts, invalid connections flash rose. |
| 11 | Solid drawing in 3D | Keep consistent light/depth logic. | One light source: lighter = higher, everywhere. |
| 12 | Overall appeal | The sum of the details. | — |
| 13 | Fitts's law | Time to hit a target depends on its size and distance. | Full-width sidebar rows, 36–40px hit targets, primary action at the top right of each page. |
| 14 | Bigger hit areas | Make the clickable area larger than the visual. | Icon buttons are 32px squares around 16px icons; canvas handles have an enlarged invisible hit ring. |
| 15 | Hick's law | More choices = slower decisions. | Basic nav grouped into 3 short sections; block menu is searchable. |
| 16 | Miller's law | People hold about 7±2 items in working memory. | KPI rows capped at 4; nav groups ≤ 4 items. |
| 17 | Doherty threshold | Respond within ~400 ms to keep flow. | Skeleton states and instant local feedback before the API answers. |
| 18 | Postel's law | Be liberal in what you accept, strict in what you emit. | Money inputs accept `$1,000.50` and `1000.5`; values are normalised before sending. |
| 19 | Jakob's law | Users expect your app to work like the ones they already use. | Canvas behaves like n8n, chart like a trading terminal, code tab like an editor. |
| 20 | Frequency of use | Put the most-used things closest. | Pending approvals surface on the Overview and as a sidebar count. |
