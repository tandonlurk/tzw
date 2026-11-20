# TZW logo concepts (paper-cut × Renaissance fresco)

Two reaching hands after Michelangelo's *Creation of Adam* (public domain), cut from layered paper
in fresco tones. The object in the gap is what TZW stands for. Each concept comes in two sizes:
`NN-name.jpg` at 1200x800 (Devpost thumbnail, 3:2) and `NN-name-sq.jpg` at 1024x1024 (avatar / icon).

| # | File | Idea |
|---|---|---|
| 01 | `01-coin` | A gold coin with a Z passes between the hands: value exchanged |
| 02 | `02-escrow-lock` | A padlock: the stake both sides lock before a challenge |
| 03 | `03-term-scroll` | A scroll: terms written down before money moves |
| 04 | `04-scales` | A balance: a fair, pre-agreed way to decide who won |
| 05 | `05-tick-wordmark` | Letter logo: the app's yellow tick is the spark; TZW in Roman capitals below |
| 06 | `06-z-spark` | Letter logo: a cut-paper Z between the fingers, inside a fresco roundel |
| 07 | `07-owner-agent` | Dark version: a human hand meets a blue "agent" hand traced with circuit nodes |
| 08 | `08-agent-network` | Three linked nodes: many owners' agents meeting each other |
| 09 | `09-chain-link` | Two interlocked links: the agreement recorded onchain |
| 10 | `10-cartouche` | Letter logo: TZW on a Renaissance cartouche, the hands touching above it |

Source: `../../animations/src/logos/` (`paper.tsx` = hands, paper filter, palettes; `Logos.tsx` =
the ten concepts). Re-render all: `cd ../../animations && npm run logos`. To change a concept, edit
its entry in `Logos.tsx`; palettes `FRESCO` and `NIGHT` are in `paper.tsx`.
