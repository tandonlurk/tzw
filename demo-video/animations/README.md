# Demo video animations (Remotion)

Full-frame animated scenes for the story in `../story-outline.md`. Uses the app's own tokens
(`web/app/globals.css` is imported directly), Inter and JetBrains Mono, 1920x1080 at 30 fps.

```sh
npm ci
npm run studio          # preview and scrub every scene in the browser
npm run render          # all scenes -> renders/NN-slug.mp4 (gitignored)
npm run render -- 08 12 # only scenes whose id starts with these prefixes
npm run typecheck
```

## Status (2026-11-20)

| Scene | File | Dur | State |
|---|---|---|---|
| 01-cold-open | `S01ColdOpen.tsx` | 4 s | built |
| 02-problem | `S02Problem.tsx` | 21 s | built (3 beats × 7 s) |
| 03-agent-to-agent | `S03AgentToAgent.tsx` | 10 s | built, continues 02's ring |
| 05-lifecycle-map | `S05LifecycleMap.tsx` | 8 s | built, collapses into the strip used by 06–08 |
| 06-discover | `S06Discover.tsx` | 10 s | built |
| 07-negotiate-guard | `S07NegotiateGuard.tsx` | 14 s | built (negotiation, then four checks) |
| 08-escrow | `S08Escrow.tsx` | 20 s | built, values from the 18 Nov live Gemini run (`docs/submission/arc.md`) |
| 10-scale | `S10Scale.tsx` | 15 s | built (split, multiply to 100× per D-73, family rule) |
| 12-firms | `S12Firms.tsx` | 18 s | built, values from `backend/README.md` "Firm demo" |
| 14-close | `S14Close.tsx` | 6 s | built |
| 04 / 09 / 11 overlays | — | — | not started: need the screen recordings and their clip times first |

Timings are set to the outline, not to a voiceover. Once the VO is recorded, stretch beats by editing
the second values in each scene (all times are seconds; `lib/anim.ts` converts to frames).

## Layout

- `src/lib/anim.ts`: timing helpers (`prog`, `fade`, `snap`, `tick`, `typed`) with the app's eases.
- `src/lib/ui.tsx`: shared components: `Stage` (canvas + env badge + Example/Replay tag), `AgentAvatar`,
  `MoneyChip`, `Pill`, `Card`, `TxChip`, `BlockGlyph`, `LifecycleStrip`, `Title`, `Caption`, `Wordmark`.
- `src/lib/icons.tsx`: lifecycle station icons. `src/lib/ring.ts`: the five-agent ring shared by 02 and 03.
- `src/Root.tsx`: one composition per scene; ids are the file prefixes the assembly step pairs on.
