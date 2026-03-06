/*
 * pingpong.cpp - Weyvelength SDK example
 *
 * Mimics a synchronous 60 Hz game loop:
 *   - The SDK callback fires on a background thread; packets are copied into
 *     a thread-safe queue immediately.
 *   - Once per frame the game thread swaps the queue out and processes every
 *     pending packet — no network calls happen mid-frame.
 *
 * Build (MSVC, from sdk/):
 *   cl /EHsc /std:c++17 /I. examples\pingpong.cpp target\debug\wl_sdk.dll.lib /Fe:pingpong.exe
 *
 * Build (GCC/Clang, from sdk/):
 *   g++ -std=c++17 -I. examples/pingpong.cpp -L. -lwl_sdk -o pingpong
 */

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>
#include <mutex>
#include <queue>
#include <vector>

#ifdef _WIN32
#  include <windows.h>
static void     sleep_ms(int ms) { Sleep(ms); }
static uint64_t now_ms()
{
    static LARGE_INTEGER freq{};
    if (!freq.QuadPart) QueryPerformanceFrequency(&freq);
    LARGE_INTEGER cnt; QueryPerformanceCounter(&cnt);
    return (uint64_t)(cnt.QuadPart * 1000 / freq.QuadPart);
}
#else
#  include <unistd.h>
#  include <time.h>
static void     sleep_ms(int ms) { usleep((unsigned)ms * 1000u); }
static uint64_t now_ms()
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000 + (uint64_t)ts.tv_nsec / 1000000;
}
#endif

#include "../wl-sdk.h"

/* ── Packet queue ─────────────────────────────────────────────────────────────
 *
 * The SDK callback runs on a background IO thread. We copy the packet data
 * immediately (data is only valid during the callback) and push it onto a
 * shared queue. The game thread swaps the whole queue out once per frame —
 * holding the lock only long enough to do a pointer swap, not to process.
 */

/* Shared between the callback thread (reads ping_sent_at, writes RTT) and the
 * game thread (writes ping_sent_at when sending). Atomics make both safe. */
struct PingState {
    std::atomic<uint64_t> ping_sent_at[9] = {};
};

struct Packet {
    uint8_t              from;
    std::vector<uint8_t> data;
};

static std::mutex         g_mutex;
static std::queue<Packet> g_queue;

static void on_recv(uint8_t from, const void *data, int len, void *userdata)
{
    if (len <= 0) return;
    PingState *ps = static_cast<PingState *>(userdata);

    /* ── Background processing (runs even during sleep) ────────────────────
     * Both ping replies and RTT measurement happen instantly on the IO thread
     * the moment a packet arrives — no frame boundary wait.
     */
    const char *msg = static_cast<const char *>(data);
    if (len == 4 && memcmp(msg, "ping", 4) == 0)
    {
        wl_send(from, "pong", 4);
    }
    else if (len == 4 && memcmp(msg, "pong", 4) == 0 && from < 9)
    {
        uint64_t sent = ps->ping_sent_at[from].exchange(0);
        if (sent)
        {
            printf("[pingpong] RTT to player %d: %llu ms\n",
                   (int)from, (unsigned long long)(now_ms() - sent));
            fflush(stdout);
        }
    }

    /* ── Queue for game-thread processing ───────────────────────────────────
     * Game loop still sees every packet at frame boundaries for any
     * game-state work (input application, logging, etc.).
     */
    Packet pkt;
    pkt.from = from;
    pkt.data.assign(static_cast<const uint8_t *>(data),
                    static_cast<const uint8_t *>(data) + len);
    std::lock_guard<std::mutex> lk(g_mutex);
    g_queue.push(std::move(pkt));
}

/* Swap the shared queue into a local one — lock held only for the swap. */
static std::queue<Packet> drain_queue()
{
    std::queue<Packet> local;
    std::lock_guard<std::mutex> lk(g_mutex);
    std::swap(local, g_queue);
    return local;
}

/* ── Arg helpers ──────────────────────────────────────────────────────────── */

static int parse_int(int argc, char **argv, const char *flag, int fallback)
{
    for (int i = 1; i + 1 < argc; ++i)
        if (strcmp(argv[i], flag) == 0) return atoi(argv[i + 1]);
    return fallback;
}

static const char *find_arg(int argc, char **argv, const char *flag)
{
    for (int i = 1; i + 1 < argc; ++i)
        if (strcmp(argv[i], flag) == 0) return argv[i + 1];
    return nullptr;
}

/* ── Config parser ────────────────────────────────────────────────────────── */

static int collect_player_ids(const char *cfg, uint8_t *out, int max)
{
    int n = 0;
    const char *p = cfg;
    while (n < max) {
        p = strstr(p, "\"playerId\"");
        if (!p) break;
        p += 10;
        while (*p == ' ' || *p == ':') ++p;
        int id = atoi(p);
        if (id > 0) out[n++] = (uint8_t)id;
        while (*p >= '0' && *p <= '9') ++p;
    }
    return n;
}

/* ── Main ─────────────────────────────────────────────────────────────────── */

int main(int argc, char **argv)
{
    int port      = parse_int(argc, argv, "--wl-udp-port",  0);
    int player_id = parse_int(argc, argv, "--wl-player-id", 0);

    if (port == 0) {
        fprintf(stderr, "usage: pingpong --wl-udp-port <port> --wl-player-id <id> [--wl-config <json>]\n");
        return 1;
    }

    uint8_t peers[8]; int peer_count = 0;
    const char *cfg = find_arg(argc, argv, "--wl-config");
    if (cfg) {
        uint8_t all_ids[8];
        int total = collect_player_ids(cfg, all_ids, 8);
        for (int i = 0; i < total; ++i)
            if (all_ids[i] != (uint8_t)player_id)
                peers[peer_count++] = all_ids[i];
    }

    PingState ps;
    wl_set_callback(on_recv, &ps);

    if (wl_init(port, (uint8_t)player_id) != 0) {
        fprintf(stderr, "[pingpong] wl_init failed: %s\n", wl_last_error());
        return 1;
    }

    printf("[pingpong] player %d, %d peer(s)\n", (int)wl_local_player_id(), peer_count);
    fflush(stdout);

    uint64_t last_ping_ms = 0;

    /* Game-thread packet handler — RTT already printed from callback,
     * so this is just a hook for any game-state work per packet. */
    auto process_packets = [&](std::queue<Packet> packets) {
        while (!packets.empty())
        {
            Packet &pkt = packets.front();

            char msg[1401];
            int  len = (int)pkt.data.size() < 1400 ? (int)pkt.data.size() : 1400;
            memcpy(msg, pkt.data.data(), (size_t)len);
            msg[len] = '\0';

            if (strcmp(msg, "ping") == 0)
            {
                printf("[pingpong] [frame] recv ping from player %d\n", (int)pkt.from);
                fflush(stdout);
            }
            else if (strcmp(msg, "pong") == 0)
            {
                printf("[pingpong] [frame] recv pong from player %d\n", (int)pkt.from);
                fflush(stdout);
            }

            packets.pop();
        }
    };

    for (;;)
    {
        /* ── Drain 1: start of frame ────────────────────────────────────────
         * Packets queued while we were sleeping. Process before game logic
         * so this frame's simulation sees all inputs that arrived last frame.
         */
        process_packets(drain_queue());

        /* ── Game logic: send "ping" every 2 s ──────────────────────────── */
        uint64_t now = now_ms();
        if (now - last_ping_ms >= 20 && peer_count > 0)
        {
            last_ping_ms = now;
            for (int i = 0; i < peer_count; ++i)
            {
                uint8_t id = peers[i];
                if (wl_send(id, "ping", 4) == 0) {
                    ps.ping_sent_at[id].store(now_ms());
                    printf("[pingpong] sent ping to player %d\n", (int)id);
                    fflush(stdout);
                }
            }
        }

        /* ── Drain 2: end of frame ──────────────────────────────────────────
         * Catch any packets that arrived while game logic ran. These will be
         * processed this frame rather than waiting until next frame.
         */
        process_packets(drain_queue());

        /* ── 60 Hz frame budget ─────────────────────────────────────────── */
        sleep_ms(16);
    }

    wl_shutdown();
    return 0;
}
