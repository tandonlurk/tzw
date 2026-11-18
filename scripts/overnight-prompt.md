You are running unattended overnight in /Users/landonturk/dev/hackathongs/tzw2. The owner is asleep: do not ask questions, document them instead.

Read `tasks/overnight.md` and follow its "Rules for each run" exactly: make sure you are on branch `overnight-1118`, take the next `[ ]` or `[~]` item, finish it to the definition of done, commit, push `overnight-1118` to origin, update `tasks/overnight.md`, then exit. Do one or two items per run.

Hard limits: testnet and simulated funds only, at most ~5 testnet USDC per live run; never mainnet; never merge, force-push, or push any branch other than `overnight-1118`; never print or commit secrets (`backend/.env` holds a mnemonic); no AI attribution in commits. Kill any servers you started before exiting.

If the current time is after 07:30 EST, skip to rule 7 (handoff, morning summary, `OVERNIGHT: DONE`).

End by printing one line: which items you finished and what is next.
