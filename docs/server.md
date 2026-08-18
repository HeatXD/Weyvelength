# Running the server

One executable, no dependencies, no state on disk. Rooms live in memory and die with the process.

```sh
./Server --port 5555
```

## Options

| Option | Default | What it does |
| --- | --- | --- |
| `--port PORT` | `5555` | TCP port to listen on |
| `--room-code-length N` | `8` | Characters in a generated room code |
| `--room-list-cooldown-ms MS` | `1000` | How often one client may ask for the room list |
| `--stun-host HOST` | `stun.l.google.com` | STUN server handed to clients; empty disables it |
| `--stun-port PORT` | `19302` | Port for the above |
| `--turn HOST:PORT:USER:PASS` | none | A TURN server; repeatable |
| `--ice-file PATH` | none | Read STUN/TURN from a file and reread it as it expires |

`--ice-file` replaces the three ice flags and lets credentials rotate while the server runs. See [ice_servers.md](ice_servers.md).

## Room codes

Drawn from `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`, leaving out `0`, `O`, `1` and `I` so a code survives being read aloud. At length 8 that is about 40 bits, and collisions are retried. A room code is the only thing protecting an unlisted room, so treat it as a secret.

## Client ids

Every connection gets a `uint32_t`, unique among connected clients and not stable across reconnects.

Ids are a 20 bit slot plus a 12 bit generation. A released slot is reissued with the generation bumped, so a stale reference to a departed client misses cleanly instead of landing on whoever inherited the slot. The generation wraps after 4096 reuses. Connections are refused past 1048575 concurrent clients.

Never persist a client id or key anything durable off one. Ban lists are per room and per connection, so a banned client rejoining under a new id is not currently stopped.

## Rate limiting

`ListRooms` is the only limited request, one per `--room-list-cooldown-ms` per client, returning `WEYVE_ROOM_ERROR_RATE_LIMITED` when asked sooner. Nothing else is limited, so put connection level protection in front if it faces the open internet.

## Logging

spdlog to stdout at debug level, flushed per message. Peer to peer signaling logs at debug, which is loud but useful while getting NAT traversal working.
