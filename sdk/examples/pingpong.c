/*
 * pingpong.c — Weyvelength SDK example
 *
 * Reads the session config (--wl-config) to find active player port slots,
 * broadcasts "ping" to all of them every 2 s, and replies "pong" to any
 * received "ping".  Designed to be launched via Weyvelength Launch Mode.
 *
 * Build (MSVC, from the sdk/ directory):
 *   cl /I. examples\pingpong.c wl-sdk.lib /Fe:pingpong.exe
 *
 * Build (GCC/Clang, from the sdk/ directory):
 *   gcc -I. examples/pingpong.c -L. -lwl-sdk -o pingpong
 *
 * Args passed automatically by the Weyvelength client on launch:
 *   --wl-udp-port  <port>       bridge port the SDK connects to
 *   --wl-player-id <id>         this client's port slot (0 = spectator)
 *   --wl-config    <json>       full session config with all member assignments
 *
 * Config JSON shape:
 *   { "game": "...", "platform": "...",
 *     "members": { "<name>": { "role": "Player", "player_id": 1 }, ... } }
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#ifdef _WIN32
#include <windows.h>
#define sleep_ms(ms) Sleep(ms)
#else
#include <unistd.h>
#define sleep_ms(ms) usleep((unsigned int)(ms) * 1000u)
#endif

#include "../wl-sdk.h"

/* ── arg helpers ──────────────────────────────────────────────────────────── */

static int parse_int(int argc, char **argv, const char *flag, int fallback)
{
    for (int i = 1; i + 1 < argc; ++i)
        if (strcmp(argv[i], flag) == 0)
            return atoi(argv[i + 1]);
    return fallback;
}

static const char *find_arg(int argc, char **argv, const char *flag)
{
    for (int i = 1; i + 1 < argc; ++i)
        if (strcmp(argv[i], flag) == 0)
            return argv[i + 1];
    return NULL;
}

/* ── config parser ────────────────────────────────────────────────────────── */

/*
 * Scan the config JSON for "player_id": N entries and collect every N > 0.
 * Returns the number of IDs found (≤ max).  No full JSON parser needed —
 * the token "player_id" only appears in the members map.
 */
static int collect_player_ids(const char *cfg, uint8_t *out, int max)
{
    int n = 0;
    const char *p = cfg;
    while (n < max)
    {
        p = strstr(p, "\"player_id\"");
        if (!p)
            break;
        p += 11; /* past "player_id" */
        while (*p == ' ' || *p == ':')
            ++p;
        int id = atoi(p);
        if (id > 0)
            out[n++] = (uint8_t)id;
        while (*p >= '0' && *p <= '9')
            ++p;
    }
    return n;
}

/* ── main ─────────────────────────────────────────────────────────────────── */

int main(int argc, char **argv)
{
    int port = parse_int(argc, argv, "--wl-udp-port", 0);
    int player_id = parse_int(argc, argv, "--wl-player-id", 0);

    printf("[pingpong] argc=%d\n", argc);
    for (int i = 0; i < argc; ++i)
        printf("[pingpong] argv[%d] = %s\n", i, argv[i]);
    fflush(stdout);

    if (port == 0)
    {
        fprintf(stderr,
                "usage: pingpong --wl-udp-port <port> --wl-player-id <id> "
                "[--wl-config <json>]\n");
        return 1;
    }

    /* Parse config to learn which port slots exist in this session. */
    uint8_t peers[8];
    int peer_count = 0;

    const char *cfg = find_arg(argc, argv, "--wl-config");
    if (cfg)
    {
        uint8_t all_ids[8];
        int total = collect_player_ids(cfg, all_ids, 8);
        /* Keep every ID except our own. */
        for (int i = 0; i < total; ++i)
            if (all_ids[i] != (uint8_t)player_id)
                peers[peer_count++] = all_ids[i];
    }

    /* Initialise the SDK. */
    if (wl_init(port, (uint8_t)player_id) != 0)
    {
        fprintf(stderr, "[pingpong] wl_init failed: %s\n", wl_last_error());
        return 1;
    }

    uint8_t my_id = wl_local_player_id();
    printf("[pingpong] started as player %d, %d remote peer(s)\n",
           (int)my_id, peer_count);
    for (int i = 0; i < peer_count; ++i)
        printf("[pingpong]   remote player %d\n", (int)peers[i]);

    time_t last_ping = 0;

    for (;;)
    {
        /* ── drain inbound packets ─────────────────────────────────────── */
        int n = 0;
        const WL_Event *events = wl_poll(&n);
        for (int i = 0; i < n; ++i)
        {
            const WL_Event *ev = &events[i];
            if (ev->kind != WL_EVENT_PACKET)
                continue;

            char msg[WL_PACKET_MAX + 1];
            int len = ev->data_len < WL_PACKET_MAX ? ev->data_len : WL_PACKET_MAX;
            memcpy(msg, ev->data, (size_t)len);
            msg[len] = '\0';

            printf("[pingpong] recv from port %d: %s\n",
                   (int)ev->from_player_id, msg);

            if (strcmp(msg, "ping") == 0)
            {
                wl_send(ev->from_player_id, "pong", 4);
                printf("[pingpong] sent pong to port %d\n",
                       (int)ev->from_player_id);
            }
        }

        /* ── send "ping" to all session peers every 2 s ───────────────── */
        time_t now = time(NULL);
        if (now - last_ping >= 2 && peer_count > 0)
        {
            last_ping = now;
            for (int i = 0; i < peer_count; ++i)
            {
                if (wl_send(peers[i], "ping", 4) == 0)
                    printf("[pingpong] sent ping to port %d\n", (int)peers[i]);
            }
        }

        sleep_ms(10);
    }

    wl_shutdown();
    return 0;
}
