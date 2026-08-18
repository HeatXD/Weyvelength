# STUN and TURN servers

STUN tells a peer how its address looks from outside, which is enough for most connections. TURN relays traffic for the rest, which costs bandwidth and is why it wants credentials.

The server hands its set to every client on connect. You never configure this on the client.

## Flags

Google's public STUN is the default, so this already works for most peers:

```sh
./Server
./Server --turn turn.example.net:3478:alice:secret   # repeatable
```

Fine for long lived credentials. For anything that rotates, use a file.

## The ice file

```sh
./Server --ice-file /etc/weyvelength/ice.conf
```

```
# rewritten by the minter
expires 1755500000
stun stun.example.net 3478
turn turn.example.net 3478 1755500000:alice aGVsbG8gd29ybGQ=
```

| Directive | Meaning |
| --- | --- |
| `expires UNIX_SECONDS` | When the credentials below stop working. Optional. |
| `stun HOST PORT` | The STUN server. At most one. |
| `turn HOST PORT USER PASS` | A TURN server. Repeat for more. |

Blank lines and `#` comments are ignored. The file replaces `--stun-host`, `--stun-port` and `--turn` entirely, and a bad one at startup is fatal.

Fields are space separated, not colon separated like `--turn`, because a coturn REST username is `<expiry>:<name>` and a colon split would tear it in half.

### Rereading

The server rereads 5 minutes before `expires`, swaps, and pushes to every connected client. Nobody is dropped, and links already built keep their old configuration since expiry only gates new allocations.

- **An unchanged file sends nothing.** The set is compared before it is broadcast.
- **A bad read never blanks working credentials.** A malformed line fails the whole file, and so does one that parses cleanly but describes no STUN and no TURN, which is what a file truncated on a line boundary looks like. The server keeps what it has and retries in 30 seconds. To run with no ice servers, leave `--ice-file` off rather than pointing it at an empty file.
- **`expires` never reaches a client.** It only says when to reread.

No `expires` line means reread every 5 minutes. An expiry already inside the margin retries every 30 seconds; one further out than an hour is capped at an hour, so an expiry written in milliseconds cannot park the reread indefinitely.

On startup and every swap the server logs the STUN host and each TURN host with its username, passwords excluded.

## Making a relay reachable

Both self hosted options below need the same three things, or peers on other networks never reach the relay.

- **A public address.** What you advertise and hand to clients must be a public IP or a name resolving to one, never a LAN address.
- **Open inbound UDP** on the listening port and the whole relay port range.
- **Behind NAT**, forward those ports and tell the relay its public address explicitly. Otherwise it advertises the private address it sees on its own interface, and every outside peer is handed a candidate it cannot route to. This presents as the relay running fine while nothing connects.

None of this applies to STUN, which is one port and answers from wherever it is reachable.

## Your own coturn

Run it in `use-auth-secret` mode, which lets credentials rotate without a restart:

```sh
turnserver \
  --listening-port=3478 --min-port=49160 --max-port=49200 \
  --external-ip=203.0.113.10 \
  --realm=weyvelength \
  --use-auth-secret --static-auth-secret="$TURN_SECRET" \
  --no-cli --no-tls --no-dtls
```

Behind NAT, `--external-ip` takes a `PUBLIC/PRIVATE` pair, e.g. `203.0.113.10/10.0.0.5`.

Nothing is provisioned per user: coturn checks an HMAC of the username against the secret, so credentials can be minted anywhere the secret is held.

```
username = "<unix-expiry>:<any-name>"
password = base64(HMAC-SHA1(secret, username))
```

[`examples/mint-turn-credentials.sh`](examples/mint-turn-credentials.sh) does that with `sh`, `openssl` and `date`:

```sh
export TURN_SECRET=the-same-secret-coturn-runs-with
export TURN_HOSTS="203.0.113.10 3478"
export ICE_FILE=/etc/weyvelength/ice.conf
./mint-turn-credentials.sh
```

Run it at roughly half the TTL, so a fresh batch always precedes the reread margin:

```
*/30 * * * * TURN_SECRET=... /usr/local/bin/mint-turn-credentials.sh
```

It writes beside the target and renames into place. Other knobs: `TURN_TTL` (default 3600), `TURN_NAME`, `STUN_SERVER`. coturn answers STUN on the same port, so `STUN_SERVER="203.0.113.10 3478"` keeps the deployment self contained.

## Your own Violet

[Violet](https://github.com/paullouisageneau/violet) is a small STUN/TURN server by the same author as libjuice, which Weyvelength already uses for the client half of ICE. No dependencies, one binary.

```sh
violet --credentials=alice:secret --port=3478 --range=49160:49200 --external=203.0.113.10
```

or with a config file:

```
port=3478
range=49160:49200
external=203.0.113.10
credentials=alice:secret
```

```sh
violet -f /etc/violet.conf
```

Credentials are static pairs read once at startup. There is no shared secret mode and no config reload, so **rotation means restarting Violet, which drops the relayed sessions in flight**. With a long lived credential there is nothing to mint, so the flags are enough:

```sh
./Server --stun-host 203.0.113.10 --stun-port 3478 --turn 203.0.113.10:3478:alice:secret
```

## Cloudflare

Cloudflare Realtime rents you TURN, so there is no relay to run, nothing to expose and no address to advertise. Credentials come from an API rather than a shared secret, so they cannot be computed offline:

```sh
curl -X POST \
  "https://rtc.live.cloudflare.com/v1/turn/keys/$CF_TURN_KEY_ID/credentials/generate-ice-servers" \
  -H "Authorization: Bearer $CF_TURN_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"ttl": 86400}'
```

[`examples/mint-cloudflare-credentials.sh`](examples/mint-cloudflare-credentials.sh) wraps that into the same ice file, using `sh` and `curl`:

```sh
export CF_TURN_KEY_ID=... CF_TURN_API_TOKEN=...
export ICE_FILE=/etc/weyvelength/ice.conf
./mint-cloudflare-credentials.sh
```

**Only two of the six endpoints Cloudflare returns are usable**, because libjuice speaks TURN over UDP alone:

| Endpoint | Usable |
| --- | --- |
| `turn:turn.cloudflare.com:3478?transport=udp` | yes |
| `turn:turn.cloudflare.com:53?transport=udp` | yes |
| `turn:...?transport=tcp` (3478, 80) | no |
| `turns:...?transport=tcp` (5349, 443) | no |

Losing `turns:` on 443 matters: that is the endpoint that escapes restrictive corporate networks by looking like HTTPS. Port 53 partly compensates, since DNS shaped traffic passes many captive networks and the browser block Cloudflare warns about does not apply here. Both UDP ports are listed by default; `CF_TURN_PORTS=3478` drops the second.

Two other differences:

- Rotation needs a third party reachable. A failed run writes nothing and exits non zero, so the server keeps serving what it has.
- No shared secret to leak, but an API token that can mint against your account until rotated.

It is metered on relayed traffic, against the fixed cost of a box you already run.

## Choosing

| | Rotates without a restart | You run it | Notes |
| --- | --- | --- | --- |
| coturn | yes | yes | Shared secret, credentials minted anywhere |
| Violet | no | yes | Small and dependency free, static credentials |
| Cloudflare | yes | no | UDP endpoints only, metered on relayed traffic |

## Turning it off

An empty `--stun-host` disables STUN, and no `--turn` means no TURN. Peers then connect only when already directly reachable, which is occasionally what a LAN build wants.
