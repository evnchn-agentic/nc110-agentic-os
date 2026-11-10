#!/usr/bin/env bash
# Interop test of src/wg.rs against Linux kernel WireGuard, no netbook needed.
# A container (needs a Docker VM kernel with WireGuard: OrbStack and Docker Desktop have it) is the server at 10.6.0.1;
# tools/wg-interop runs the kernel's own wg.rs on the host as the client at 10.6.0.2, and the server pings it.
#
#   tools/wg-interop.sh            all scenarios; exit 0 only if every one passes
#
# Scenarios (each client run is a fresh process, i.e. a fresh boot of the agent OS):
#   basic     handshake, then 20 server pings answered through the tunnel
#   first-confirm-lost, rekey-confirm-lost
#             the first data packet after a handshake (the confirmation) is lost on the air. Linux does not send under
#             the new keys until it hears from us under them, and we only answer what we can decrypt, so without a
#             retry the tunnel is one-way until the next keepalive (25 s). Pass = at most 10 of 60 lost (the retry is every 1 s, about 4 pings).
#   rtc-back  a second boot whose RTC reads 8 h earlier than the first (HKT -> UTC). Expected to FAIL to handshake:
#             Linux drops initiations whose TAI64N is not newer than the last one it accepted, silently. Reported, not
#             counted, because the client cannot fix it on its own (see the PR / README note).
set -euo pipefail
cd "$(dirname "$0")/wg-interop"
cargo build -q --release --target "$(rustc -vV | sed -n 's/^host: //p')"
BIN=$(ls -t target/*/release/wg-interop | head -1)
NAME=nc110-wg-interop-$$
PORT=${WG_PORT:-51820}

docker run -d --rm --name "$NAME" --cap-add NET_ADMIN -p "$PORT:51820/udp" alpine:3.20 sleep 600 >/dev/null
trap 'docker rm -f "$NAME" >/dev/null 2>&1' EXIT
docker exec "$NAME" apk add -q wireguard-tools-wg >/dev/null
SK=$(docker exec "$NAME" wg genkey); CK=$(docker exec "$NAME" wg genkey)
SP=$(echo "$SK" | docker exec -i "$NAME" wg pubkey); CP=$(echo "$CK" | docker exec -i "$NAME" wg pubkey)
hex() { echo "$1" | base64 -d | od -An -tx1 | tr -d ' \n'; }
docker exec "$NAME" sh -c "ip link add wg0 type wireguard && echo $SK > /k && wg set wg0 listen-port 51820 private-key /k \
  peer $CP allowed-ips 10.6.0.2/32 && ip addr add 10.6.0.1/24 dev wg0 && ip link set wg0 up"
client() { "$BIN" "$(hex "$CK")" "$(hex "$SP")" 127.0.0.1 "$PORT" 10.6.0.2/24 "$@"; }
# answered pings out of $2 sent at 0.25 s, after $1 s
pings() { sleep "$1"; { docker exec "$NAME" ping -q -W1 -i 0.25 -c "$2" 10.6.0.2 || true; } | sed -n 's/.*, \([0-9]*\) packets received.*/\1/p'; }
fail=0

echo "== basic"
client 8 > target/basic.log 2>&1 & got=$(pings 1 20); wait
grep -E '^(up|final):' target/basic.log
echo "server pings answered: $got/20"; [ "$got" -ge 19 ] || { echo "FAIL basic"; fail=1; }

echo "== first-confirm-lost"
# the confirmation of the very first handshake is dropped; pings run t=2..17 s
DROP_CONFIRM=1 client 19 > target/first.log 2>&1 & got=$(pings 2 60); wait
grep -E '^(up|final):|dropped our' target/first.log
echo "server pings answered: $got/60"; [ "$got" -ge 50 ] || { echo "FAIL first-confirm-lost"; fail=1; }

echo "== rekey-confirm-lost"
# pings run t=2..17 s; the re-handshake is at t=5 and its confirmation is dropped
REKEY_AT=5 DROP_CONFIRM=2 client 19 > target/rekey.log 2>&1 & got=$(pings 2 60); wait
grep -E '^(up|final):|dropped our|uptime clock' target/rekey.log
echo "server pings answered: $got/60"; [ "$got" -ge 50 ] || { echo "FAIL rekey-confirm-lost"; fail=1; }

echo "== rtc-back (informational)"
docker exec "$NAME" sh -c 'ip link set wg0 down; ip link set wg0 up'  # forget the timestamps of the runs above
RTC_OFFSET=28800 client 3 > target/rtc1.log 2>&1; grep '^up:' target/rtc1.log | sed 's/^/boot 1, RTC on HKT: /'
RTC_OFFSET=0 client 8 > target/rtc2.log 2>&1; grep '^up:' target/rtc2.log | sed 's/^/boot 2, RTC on UTC: /'
grep '^final:' target/rtc2.log

exit $fail
