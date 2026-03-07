# Weyvelength SDK — Integration Guide

## Quickstart

```c
int main(int argc, char **argv) {
    int     port   = parse_int(argc, argv, "--wl-udp-port",  0);
    uint8_t player = parse_int(argc, argv, "--wl-player-id", 0);

    if (wl_init(port, player) != 0) {
        fprintf(stderr, "wl_init: %s\n", wl_last_error());
        return 1;
    }

    while (running) {
        // Drain all inbound packets.
        uint8_t from;
        uint8_t buf[1400];
        int len;
        while ((len = wl_recv(&from, buf, sizeof(buf))) > 0)
            handle_packet(from, buf, len);

        game_tick();
        wl_send(peer_id, state, sizeof(state));
        sleep_ms(16);
    }

    wl_shutdown();
}
```

---

## Launch args

Weyvelength passes these to your executable automatically:

| Arg | Value |
|---|---|
| `--wl-udp-port <n>` | Pass to `wl_init` |
| `--wl-player-id <n>` | Your slot (1–8; 0 = spectator) |
| `--wl-config <json>` | Session config with all player assignments |

```json
{
  "game": "SuperGame.rom",
  "platform": "MyPlatform",
  "gamesFolder": "C:/Games/SNES",
  "members": {
    "Alice": { "role": "Player", "playerId": 1 },
    "Bob":   { "role": "Player", "playerId": 2 }
  }
}
```

Parse `playerId` to build your peer list. IDs are stable for the session.

---

## Emulator integration

`--wl-config` carries everything needed to boot the session:

- `game` — ROM filename (e.g. `"SuperGame.rom"`)
- `platform` — platform name from the Weyvelength launch mode (e.g. `"SNES"`)
- `gamesFolder` — absolute path to the games directory

Full ROM path is `gamesFolder + "/" + game`.

```c
// Minimal config parse — no JSON library needed
static void get_json_string(const char *json, const char *key, char *out, int out_size)
{
    char search[64];
    snprintf(search, sizeof(search), "\"%s\"", key);
    const char *p = strstr(json, search);
    if (!p) return;
    p += strlen(search);
    while (*p == ' ' || *p == ':') ++p;
    if (*p != '"') return;
    ++p;
    int i = 0;
    while (*p && *p != '"' && i < out_size - 1) out[i++] = *p++;
    out[i] = '\0';
}

int main(int argc, char **argv) {
    const char *cfg    = find_arg(argc, argv, "--wl-config");
    int         port   = parse_int(argc, argv, "--wl-udp-port",  0);
    int         player = parse_int(argc, argv, "--wl-player-id", 0);

    char game[256] = {0}, platform[64] = {0}, games_folder[512] = {0};
    if (cfg) {
        get_json_string(cfg, "game",        game,         sizeof(game));
        get_json_string(cfg, "platform",    platform,     sizeof(platform));
        get_json_string(cfg, "gamesFolder", games_folder, sizeof(games_folder));
    }

    char rom_path[768];
    snprintf(rom_path, sizeof(rom_path), "%s/%s", games_folder, game);

    emulator_load_rom(rom_path);

    if (wl_init(port, (uint8_t)player) != 0) {
        fprintf(stderr, "wl_init: %s\n", wl_last_error());
        return 1;
    }
    // ...
}
```

---

## API

```c
int         wl_init(int bridge_port, uint8_t local_player_id); // 0 on success
void        wl_shutdown(void);
uint8_t     wl_local_player_id(void);
const char *wl_last_error(void);

// Receive one packet. Returns bytes written (>0), 0 = empty, -1 = error.
// Call in a loop each tick until it returns 0.
int wl_recv(uint8_t *from_player_id, void *buf, int buf_len);

// Send immediately. Returns 0 on success.
int wl_send(uint8_t to_player_id, const void *data, int data_len);
```

Packets are unreliable and unordered, max 1400 bytes.
