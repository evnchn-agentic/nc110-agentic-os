#!/usr/bin/env bash
# NC110 pane brief step 1: does the TCO watchdog reset this board, or does the Phoenix SMI handler swallow it?
# Run with Bash timeout 600000 (reset path: ~65 s + up to 300 s wait).
# Run ONLY after evnchn typed "go 1" in the NC110 pane, with `hw claim nc110:host` held. Slot A, Linux only.
# Without GO=1 it runs the read-only preflight and stops.
# Read-only on /boot and grubenv. The netbook is not rebooted by us; a reset can only come from the TCO.
#
# Method: Slot A's kernel pings the running TCO itself (iTCO_wdt WDOG_HW_RUNNING keepalive; sysfs state=inactive).
# Opening /dev/watchdog hands the ping duty to userspace; our holder then never writes. iTCO v2 with TMR=30 s
# resets on the 2nd expiry, about 60 s after the open. If the box is still up on the SAME boot at SWALLOW_AT,
# the firmware swallows the timeouts, and the holder closes with the magic 'V' (timer halted, no reset).
# Verdicts: RESET = boot_id changed; SWALLOW = same boot, holder alive and open the whole window, countdown
# reached ~0; VOID = anything else (nothing measured). Every non-RESET exit asks the holder to magic-close
# and VERIFIES it (done file + sysfs inactive + TMR_HLT set) before claiming the box is safe.
# Holder self-disarm: magic-close on the stop flag, on TERM/INT/HUP, or after HOLD_MAX s. SIGKILL of the
# holder cannot be covered: that is an unclean close and the TCO may still reset the box later.
set -u
OUT=${OUT:-$HOME/party/nc110-m1/step1-$(date +%Y%m%dT%H%M%S)}
HOST=root@192.168.50.118
HOLD_MAX=${HOLD_MAX:-120}
SWALLOW_AT=${SWALLOW_AT:-100}
[[ $HOLD_MAX =~ ^[1-9][0-9]*$ && $SWALLOW_AT =~ ^[1-9][0-9]*$ ]] && [ "$SWALLOW_AT" -ge 90 ] && [ "$HOLD_MAX" -ge $((SWALLOW_AT + 15)) ] \
  && [ "$HOLD_MAX" -le 300 ] || { echo "need 90 <= SWALLOW_AT and SWALLOW_AT+15 <= HOLD_MAX <= 300"; exit 2; }
mkdir -p "$OUT"
ssh_nb() { ssh -F /dev/null -o BatchMode=yes -o ConnectTimeout=5 -o ServerAliveInterval=2 -o ServerAliveCountMax=2 \
  -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
  -i "$HOME/.ssh/id_ed25519_homelab" "$HOST" "$@"; }
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$OUT/run.log"; }
W=/sys/class/watchdog/watchdog0
REGS='p() { printf "%s=%s " "$1" "$(dd if=/dev/port bs=1 skip=$(($2)) count=$3 2>/dev/null | od -An -tx$3 | tr -d " ")"; }
p RLD 0x1060 2; p TCO1_STS 0x1064 2; p TCO2_STS 0x1066 2; p TCO1_CNT 0x1068 2; p TMR 0x1072 2; p SMI_EN 0x1030 4
w='$W'; echo "state=$(cat $w/state) timeleft=$(cat $w/timeleft 2>/dev/null) boot=$(cat $w/bootstatus) up=$(cut -d" " -f1 /proc/uptime) bid=$(cat /proc/sys/kernel/random/boot_id)"'

# Ask the holder to close and prove it closed cleanly. Returns 0 only on proof.
disarm() {
  local i r
  for i in 1 2 3 4 5 6 7 8 9 10; do
    r=$(ssh_nb "touch /dev/shm/nc110-wdt-stop; sleep 1; cat /dev/shm/nc110-wdt-done 2>/dev/null; $REGS" 2>/dev/null) || { sleep 2; continue; }
    echo "$r" >> "$OUT/03-disarm.txt"
    # TCO1_CNT bit 11 (TMR_HLT) set: 4-digit hex, 2nd digit from the left in 8-f.
    if grep -q '^closed-' <<<"$r" && grep -q 'state=inactive' <<<"$r" && grep -Eq 'TCO1_CNT=[0-9a-f][89a-f][0-9a-f]{2} ' <<<"$r"; then
      log "disarm verified: holder closed with V, watchdog inactive, TMR_HLT set"; return 0
    fi
    sleep 2
  done
  log "DISARM NOT VERIFIED: the TCO may still reset the box; see $OUT/03-disarm.txt"; return 1
}
on_exit() { [ -n "${ARMED:-}" ] && disarm; }

# 1. Preflight, read-only. Any surprise stops here before /dev/watchdog is opened.
ssh_nb 'hostname; awk "\$2==\"/\"{print \$1}" /proc/mounts; grub-editenv /boot/grub/grubenv list;
  for c in pidof setsid dd od; do command -v $c >/dev/null && echo have_$c; done;
  echo "pid_watchdog=$(pidof watchdog)";
  ls -l /proc/[0-9]*/fd >/dev/null 2>&1 && echo fdlist=ok;
  echo "dev_watchdog_holders=$(ls -l /proc/[0-9]*/fd 2>/dev/null | grep -c "/dev/watchdog")";
  f=/data/slotlab/fastinit-return.pid; [ -f $f ] && kill -0 "$(cat $f)" 2>/dev/null && echo return_timer=armed || echo return_timer=none;
  touch /dev/shm/.nc110w && rm /dev/shm/.nc110w && echo shm=ok;
  echo "identity=$(cat '$W'/identity) nowayout=$(cat '$W'/nowayout) timeout=$(cat '$W'/timeout)";
  /usr/local/sbin/slotlab-status 2>/dev/null; sha256sum /boot/grub/grub.cfg /boot/grub/grubenv' > "$OUT/00-preflight.txt" 2>&1
ssh_nb "$REGS" >> "$OUT/00-preflight.txt" 2>&1
cat "$OUT/00-preflight.txt"
pf=$(cat "$OUT/00-preflight.txt")
for want in nc110 /dev/sda2 saved_entry=0 have_pidof have_setsid have_dd have_od pid_watchdog= fdlist=ok \
    dev_watchdog_holders=0 return_timer=none shm=ok "identity=iTCO_wdt nowayout=0 timeout=30"; do
  grep -qx -- "$want" <<<"$pf" || { log "preflight: missing '$want': STOP, nothing armed"; exit 2; }
done
grep -q 'next_entry=.' <<<"$pf" && { log "preflight: next_entry set: STOP"; exit 2; }
grep -q 'TMR=0032 .*state=inactive' <<<"$pf" || { log "preflight: TMR/state differ: STOP"; exit 2; }
pgrep -f nc110-slotlab >/dev/null && { log "preflight: a Mac-side slotlab run is active: STOP"; exit 2; }
BID0=$(grep -o 'bid=[0-9a-f-]*' <<<"$pf" | cut -d= -f2)
[ -n "$BID0" ] || { log "preflight: no boot_id: STOP"; exit 2; }
[ "${GO:-}" = 1 ] || { log "preflight OK (boot $BID0); rerun with GO=1 to open /dev/watchdog"; exit 0; }

# 2. Holder: open, never write, magic-close on stop flag / TERM / INT / HUP / HOLD_MAX. setsid so an ssh drop
#    cannot HUP it. It writes 'opened' only after the open succeeded.
trap on_exit EXIT; ARMED=1
ssh_nb "rm -f /dev/shm/nc110-wdt-*; sync; setsid sh -c '
  exec 3>/dev/watchdog || exit 1
  bye() { printf V >&3; exec 3>&-; echo closed-\$1 > /dev/shm/nc110-wdt-done; exit 0; }
  trap \"bye signal\" TERM INT HUP
  echo \$\$ > /dev/shm/nc110-wdt-pid; cat /proc/sys/kernel/random/boot_id > /dev/shm/nc110-wdt-opened; i=0
  while [ \$i -lt $HOLD_MAX ] && [ ! -e /dev/shm/nc110-wdt-stop ]; do sleep 1; i=\$((i+1)); done
  bye after-\$i' </dev/null >/dev/null 2>&1 &
  sleep 2; cat /dev/shm/nc110-wdt-opened 2>/dev/null; cat $W/state" > "$OUT/01-start.txt" 2>&1
cat "$OUT/01-start.txt"
grep -qx "$BID0" "$OUT/01-start.txt" && grep -qx active "$OUT/01-start.txt" \
  || { log "VERDICT VOID: holder did not ack an open on boot $BID0"; exit 1; }
T0=$(date +%s); log "holder open acked (t0 ~2 s after open), expect reset ~58 s, swallow check at t=$SWALLOW_AT s"

# 3. Watch: registers once a second over one ssh, plus ping from the Mac. Ping loss = loss of CONTACT only.
ssh_nb "while :; do $REGS; sleep 1; done" > "$OUT/02-regs.txt" 2>&1 &
RP=$!
last_up=$T0; lost=
while :; do
  now=$(date +%s); t=$((now - T0))
  if ping -c1 -t1 192.168.50.118 >/dev/null 2>&1; then last_up=$now; fi
  if [ $((now - last_up)) -ge 5 ]; then log "contact lost at t=$((last_up - T0)) s"; lost=1; break; fi
  [ $t -ge "$SWALLOW_AT" ] && break
  sleep 1
done
kill "$RP" 2>/dev/null; wait "$RP" 2>/dev/null
tail -3 "$OUT/02-regs.txt"

# 4a. Still in contact at SWALLOW_AT: check the window was a real experiment, then disarm.
if [ -z "$lost" ]; then
  st=$(ssh_nb "ls /dev/shm/nc110-wdt-done 2>/dev/null; cat /proc/sys/kernel/random/boot_id; cat $W/state;
    ls -l /proc/\$(cat /dev/shm/nc110-wdt-pid)/fd/3 2>/dev/null | grep -q /dev/watchdog && echo holder_fd=ok" 2>&1)
  echo "$st" > "$OUT/04-at-deadline.txt"
  mintl=$(grep -o 'timeleft=[0-9]*' "$OUT/02-regs.txt" | cut -d= -f2 | sort -n | head -1)
  nact=$(grep -c 'state=active' "$OUT/02-regs.txt"); nall=$(grep -c 'state=' "$OUT/02-regs.txt")
  if ! grep -q nc110-wdt-done <<<"$st" && grep -qx "$BID0" <<<"$st" && grep -qx active <<<"$st" && grep -qx holder_fd=ok <<<"$st" \
     && [ "$nall" -ge $((SWALLOW_AT - 15)) ] && [ "$nact" -eq "$nall" ] && [ -n "$mintl" ] && [ "$mintl" -le 2 ]; then
    v="SWALLOW: same boot, our holder still owns /dev/watchdog at the deadline, $nact/$nall samples active, timeleft reached $mintl, no reset by t=$SWALLOW_AT s"
  else
    v="VOID: done=$(grep -c nc110-wdt-done <<<"$st") samples=$nact/$nall active min_timeleft=${mintl:-none}. Nothing measured"
  fi
  if disarm; then ARMED=; log "VERDICT $v. Slot A now runs with the TCO halted until next boot"; exit 0; fi
  log "VERDICT $v, BUT disarm unverified"; exit 4   # EXIT trap retries the disarm once more
fi

# 4b. Contact lost: wait for the box, then decide by boot_id.
log "waiting up to 5 min for the box"
for _ in $(seq 60); do
  if ssh_nb "hostname; awk '\$2==\"/\"{print \$1}' /proc/mounts; grub-editenv /boot/grub/grubenv list; $REGS;
      sha256sum /boot/grub/grub.cfg /boot/grub/grubenv; dmesg | grep -i -e tco -e watchdog" > "$OUT/05-after.txt" 2>&1; then
    cat "$OUT/05-after.txt"
    bid=$(grep -Eo 'bid=[0-9a-f]{8}-[0-9a-f-]{27}' "$OUT/05-after.txt" | cut -d= -f2)
    if [ -z "$bid" ] || [ "$bid" = "$BID0" ]; then
      log "boot_id '${bid:-unread}' is not a new boot: no reset proven. VERDICT VOID"; exit 1   # EXIT trap disarms
    fi
    ARMED=
    log "VERDICT RESET: new boot after $(( $(date +%s) - T0 )) s from t0 (check root /dev/sda2 and sha256 = preflight)"; exit 0
  fi
  sleep 5
done
ARMED=
log "NOT BACK within 5 min: STOP, do not retry, card evnchn: NC110 needs a power cycle"; exit 3
