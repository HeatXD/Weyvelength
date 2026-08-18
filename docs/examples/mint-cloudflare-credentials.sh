#!/bin/sh
#
# Fetches short lived TURN credentials from Cloudflare Realtime and writes the
# ice file that weyvelength_server reads with --ice-file. Run it from cron or a
# systemd timer at roughly half the TTL, so a fresh batch is always in place
# before the server reaches its reread margin.
#
#   0 */6 * * * /usr/local/bin/mint-cloudflare-credentials.sh
#
# Create a TURN key in the Cloudflare dashboard to get the key id and token:
# https://developers.cloudflare.com/realtime/turn/
#
# Unlike coturn's shared secret, these credentials are minted by Cloudflare and
# cannot be computed locally, so this script needs network access every time it
# runs. That is fine: a failed run writes nothing, and the server keeps serving
# the credentials it already has.
set -eu

KEY_ID="${CF_TURN_KEY_ID:?set CF_TURN_KEY_ID to your Cloudflare TURN key id}"
TOKEN="${CF_TURN_API_TOKEN:?set CF_TURN_API_TOKEN to that key's API token}"
TTL="${CF_TURN_TTL:-86400}"                    # how long the credentials stay valid
OUT="${ICE_FILE:-/etc/weyvelength/ice.conf}"
API="${CF_API_BASE:-https://rtc.live.cloudflare.com/v1}"

TURN_HOST="${CF_TURN_HOST:-turn.cloudflare.com}"
STUN="${CF_STUN_SERVER:-stun.cloudflare.com 3478}"

# Only UDP is usable: libjuice speaks TURN over UDP alone, so Cloudflare's TCP
# and TLS endpoints cannot be reached from here. Port 53 is a second UDP entry
# that often survives networks which block 3478. Cloudflare warns browsers
# block it, which does not apply to us. Set CF_TURN_PORTS=3478 to drop it.
PORTS="${CF_TURN_PORTS:-3478 53}"

RESPONSE=$(curl -fsS -X POST \
	"$API/turn/keys/$KEY_ID/credentials/generate-ice-servers" \
	-H "Authorization: Bearer $TOKEN" \
	-H "Content-Type: application/json" \
	-d "{\"ttl\": $TTL}")

# Pulled out with sed rather than jq, to keep the dependencies to sh and curl.
# Both fields are hex, so there is no escaping to worry about. Scanning for the
# first match works whether iceServers is an array or a single object.
field() {
	printf '%s' "$RESPONSE" | tr ',{}' '\n\n\n' \
		| sed -n 's/.*"'"$1"'"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1
}

USERNAME=$(field username)
CREDENTIAL=$(field credential)

if [ -z "$USERNAME" ] || [ -z "$CREDENTIAL" ]; then
	echo "cloudflare returned no credentials, leaving $OUT alone" >&2
	exit 1
fi

EXPIRES=$(( $(date +%s) + TTL ))

# Written beside the target and moved into place, so the server never reads a
# half finished file.
TMP=$(mktemp "$OUT.XXXXXX")
trap 'rm -f "$TMP"' EXIT

{
	echo "# generated $(date -u +%Y-%m-%dT%H:%M:%SZ) from cloudflare, do not edit"
	echo "expires $EXPIRES"
	echo "stun $STUN"
	for port in $PORTS; do
		echo "turn $TURN_HOST $port $USERNAME $CREDENTIAL"
	done
} > "$TMP"

chmod 0640 "$TMP"
mv "$TMP" "$OUT"
trap - EXIT
