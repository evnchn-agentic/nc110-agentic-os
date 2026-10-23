#!/usr/bin/env bash
# Soak the NC110 agent OS: every 3 min an authenticated ping + stats over bmctl (also keeps the 10-min idle reset
# away, so any reboot seen here is a real event). Log: proof/nc110-soak.log. Stdout = events only (for a Monitor):
# START, REBOOT (boot id changed: unexpected unless we asked), LOST (n consecutive failures), HOURLY summary.
cd "$(dirname "$0")/.."
LOG=proof/nc110-soak.log; B="tools/.venv/bin/python tools/bmctl.py"
t0=$(date +%s); last_boot=""; fails=0; reboots=0; oks=0; next_hour=$((t0 + 3600))
echo "$(date '+%F %T') START soak" | tee -a "$LOG"
while true; do
  now=$(date +%s)
  if r=$($B stats 2>/dev/null) && p=$($B ping 2>/dev/null); then
    bid=$($B logall 2>/dev/null | grep -o 'boot id [0-9a-f]*' | head -1 | cut -d' ' -f3)
    up=$(sed -n 's/.*uptime_ms=\([0-9]*\).*/\1/p' <<<"$p")
    echo "$(date '+%F %T') ok uptime_ms=$up boot=$bid $r" >> "$LOG"
    [ $fails -ge 2 ] && echo "$(date '+%F %T') RECOVERED after $fails failed checks, uptime_ms=$up" | tee -a "$LOG"
    if [ -n "$last_boot" ] && [ "$bid" != "$last_boot" ]; then
      reboots=$((reboots + 1)); echo "$(date '+%F %T') REBOOT boot id $last_boot -> $bid (uptime_ms=$up)" | tee -a "$LOG"
    fi
    last_boot=$bid; fails=0; oks=$((oks + 1))
  else
    fails=$((fails + 1)); echo "$(date '+%F %T') fail #$fails" >> "$LOG"
    [ $fails = 2 ] && echo "$(date '+%F %T') LOST 2 consecutive checks failed" | tee -a "$LOG"
    [ $fails = 10 ] && echo "$(date '+%F %T') LOST 30 min: may need evnchn (power cycle)" | tee -a "$LOG"
  fi
  if [ "$now" -ge "$next_hour" ]; then
    echo "$(date '+%F %T') HOURLY $(( (now - t0) / 3600 )) h: ok=$oks reboots=$reboots current_boot=$last_boot" | tee -a "$LOG"
    next_hour=$((next_hour + 3600))
  fi
  sleep 180
done
