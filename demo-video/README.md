# Demo video

Working folder for the 2–3 minute submission video. The plan (run sheet, animation specs, shot list,
honesty flags): `animation-plan.md`. The assembly script comes later; this file fixes the naming it
will rely on.

## Folders

| Folder | Who fills it | What goes in |
|---|---|---|
| `voiceover/` | owner | Recorded voiceover, one file per segment |
| `screen-recordings/` | owner | Screen captures of the real app, one or more clips per segment |
| `animations/` | agent (later) | Animation source; rendered clips go in `animations/renders/` |
| `assets/` | agent / owner | Logos, fonts, shared styles (small files only) |
| `output/` | assembly script | Final render. Gitignored |

Media files (`.mov .mp4 .webm .wav .mp3 .m4a` and similar) are gitignored everywhere under
`demo-video/`; keep a copy outside git (or in shared storage).

## File naming

Every media file starts with its segment number and slug from the run sheet in `animation-plan.md`,
so files sort in script order and the assembly step can pair them by prefix:

```
NN-slug[-part][.overlay].ext
```

- `NN` is the two-digit segment number (`01`…`11`); `slug` is the segment slug exactly as in the
  run sheet (lowercase, hyphens). Example: `06-trust-live`.
- **Voiceover:** `voiceover/NN-slug.wav` (48 kHz WAV preferred, `.m4a` fine). One final take per
  segment. Keep spare takes as `NN-slug.take2.wav`; the assembly uses only the file without `.takeN`.
- **Screen recordings:** `screen-recordings/NN-slug-a.mov`, `-b`, `-c`… when a segment has several
  clips (letters = play order, matching the shot list). A single clip needs no letter.
  Record at 1920x1080 (browser window sized so the app fills the frame), 30 or 60 fps.
- **Animations (rendered):** `animations/renders/NN-slug.mp4` for full-frame clips;
  `animations/renders/NN-slug.overlay.mov` (ProRes 4444 with alpha) for overlays that sit on top of
  the screen recording with the same `NN-slug`.
- To reorder segments, renumber the files; the assembly follows the numbers, not the plan.

## Before you record

Follow the "Screen-recording shot list" in `animation-plan.md`. It says which database state each
segment needs and which commands from `docs/demo.md` / `backend/README.md` produce it. Keep the
sidebar footer ("Simulated and Arc testnet funds only…") and the env badges in frame.
