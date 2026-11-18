#!/bin/zsh
# Unattended overnight loop: one fresh `claude -p` run per checklist chunk until
# 08:00 EST or until tasks/overnight.md ends with "OVERNIGHT: DONE".
# A run cut off by a usage limit simply exits; the loop retries every 15 min.
# Logs: logs/overnight/ (gitignored). Stop early: kill the PID in logs/overnight/pid.

cd "${0:A:h}/.." || exit 1
unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT   # don't look like a nested session
export PATH="$HOME/.cargo/bin:$HOME/.foundry/bin:/opt/homebrew/opt/postgresql@17/bin:$PATH"

END=$(TZ=America/New_York date -j -f "%Y-%m-%d %H:%M" "${OVERNIGHT_END:-2026-11-18 08:00}" +%s)
LOGDIR=logs/overnight
mkdir -p "$LOGDIR"
echo $$ > "$LOGDIR/pid"

caffeinate -dims -w $$ &   # keep the Mac awake for as long as this script lives

i=0
while [ "$(date +%s)" -lt "$END" ] && ! tail -n 3 tasks/overnight.md | grep -q "OVERNIGHT: DONE"; do
  i=$((i + 1))
  log="$LOGDIR/run-$(date +%H%M)-$i.log"
  echo "[$(date)] run $i start" | tee -a "$LOGDIR/loop.log"
  claude -p "$(cat scripts/overnight-prompt.md)" --permission-mode auto > "$log" 2>&1
  echo "[$(date)] run $i exit=$? $(tail -n 1 "$log" | cut -c1-200)" | tee -a "$LOGDIR/loop.log"
  [ "${OVERNIGHT_ONCE:-0}" = 1 ] && break
  if grep -qiE "usage limit|rate limit|limit reached|resets" "$log"; then
    sleep 900   # wait for the limit window to reset
  else
    sleep 30
  fi
done
echo "[$(date)] loop finished after $i runs" | tee -a "$LOGDIR/loop.log"
