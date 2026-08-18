#!/bin/sh
#
# Mints short lived coturn REST credentials and writes the ice file that
# weyvelength_server reads with --ice-file. Run it from cron or a systemd
# timer at roughly half the TTL, so a fresh batch is always in place before
# the server reaches its reread margin.
#
#   */30 * * * * /usr/local/bin/mint-turn-credentials.sh
#
# Needs the same shared secret coturn runs with:
#
#   turnserver --use-auth-secret --static-auth-secret=THE_SECRET --realm=example.net
#
set -eu

SECRET="${TURN_SECRET:?set TURN_SECRET to coturn's static-auth-secret}"
TTL="${TURN_TTL:-3600}"                       # how long the credentials stay valid
NAME="${TURN_NAME:-weyvelength}"              # the label half of the username
OUT="${ICE_FILE:-/etc/weyvelength/ice.conf}"
STUN="${STUN_SERVER:-stun.l.google.com 19302}"
TURN_HOSTS="${TURN_HOSTS:-turn.example.net 3478}"  # "host port", one pair per line

EXPIRES=$(( $(date +%s) + TTL ))
USERNAME="$EXPIRES:$NAME"
PASSWORD=$(printf '%s' "$USERNAME" | openssl dgst -sha1 -hmac "$SECRET" -binary | openssl base64 -A)

# Written beside the target and moved into place, so the server never reads a
# half finished file. It tolerates one anyway, but there is no reason to make
# it retry when rename is free.
TMP=$(mktemp "$OUT.XXXXXX")
trap 'rm -f "$TMP"' EXIT

{
	echo "# generated $(date -u +%Y-%m-%dT%H:%M:%SZ), do not edit"
	echo "expires $EXPIRES"
	echo "stun $STUN"
	echo "$TURN_HOSTS" | while read -r host port; do
		[ -n "$host" ] || continue
		echo "turn $host $port $USERNAME $PASSWORD"
	done
} > "$TMP"

chmod 0640 "$TMP"
mv "$TMP" "$OUT"
trap - EXIT
