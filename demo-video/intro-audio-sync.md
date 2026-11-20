# Intro animation audio sync

Prepared for integration with the screen-capture walkthrough after its UI edits are finished. Keep the original recordings in `intro animation audios/`; the folder is intentionally separate from the final narration and screen captures. The assembly should use the current minimal Remotion renders (`animations/renders/*-min.mp4`) and preserve the existing screen-recording section as one later block.

## Requested intro order and audio cues

| Order | Animation | Audio file | Sync instruction |
|---:|---|---|---|
| 1 | `02-problem-min` | `02-problem + brief pause in front.m4a` | Start at 0. Keep the leading pause; it is intentional. |
| 2 | `01-cold-open-min` | `01 cold open mind.m4a` | Start at 0. Place after scene 02 as requested, even though the scene number is lower. |
| 3 | `03-agent-to-agent-min` | `03 agent to agent min.m4a` | Start at 0. |
| 4 | `05-lifecycle-map-min` | `05 lifecycle map min sped up plus 08 escrow after 4.5 seconds.m4a` | Start with scene 05. At audio time 4.5 s, cut to `08-escrow-min` and let scene 08 carry the remainder of this narration. Do not replay the same audio on scene 08. |
| 5 | `10-scale-min` | `10 scale min startijg at 2 secs.m4a` | Start the audio at 2.0 s; keep the scene at its first frame during that lead-in. |
| 6 | `12-firms-min` | `12 firms min.m4a` | Start at 0. |
| Ending | — | — | End the video after the screen-capture walkthrough. The owner will handle an outro separately. Do not append scene 14 or its audio. |

This maps to the owner's requested visual order `02 → 01 → 03 → 05 → 10 → 12`. The 08 escrow scene is a timed insert within the 05 audio, not a separate voiceover item. Stop after the screen-capture walkthrough; the owner will add an outro separately.

## Assembly notes

- Audio filenames are human-authored cues; avoid renaming them until their trims and offsets have been checked by listening.
- At the 05→08 transition, split the picture at exactly 4.5 seconds on the shared narration timeline. If the audio begins before the 05 picture, maintain the 2-second lead-in for scene 10 only.
- Keep cuts on the original voiceover; do not time-stretch audio without checking the speaker's cadence. If a visual render ends early, hold its last frame rather than cutting narration.
- Append the edited UI screen capture as Act 2 after scene 12, then cut the video. Do not append scene 14 or its audio.
- Media files are gitignored by `demo-video/.gitignore`; this prep document contains no copies of the audio.

## Ready-to-assemble checklist

1. Wait for the UI edits and screen-capture walkthrough to be ready.
2. Render the six minimal scenes in the order above, plus scene 08 for the 4.5-second insert.
3. Listen once to each source file and confirm the leading pause and offsets against the table.
4. Assemble animation block → screen recording block → close, keeping the total within the Devpost 2–3 minute limit.
5. Review the 05→08 cut, the paused scene 10 lead-in, audio levels, and the ending before export.

## Current first-part length estimate

The five supplied recordings in the requested intro order total about 62.6 seconds of encoded audio. The scale recording starts at 2 seconds, adding a 2-second lead-in; scene 12 has no matching file in the audio folder, so allowing its planned 18 seconds gives an intro estimate of about **82.6 seconds (1:23)**. This assumes the lifecycle/escrow audio runs 19.6 seconds total and the picture holds through the scene 12 animation. Confirm against playback during assembly; the exact voice durations may leave short holds or require trims. The 14 close recording is excluded.
