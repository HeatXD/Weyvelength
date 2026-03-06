/*
 * wl_gekko_adapter.cpp
 *
 * GekkoNet transport adapter backed by the Weyvelength SDK.
 *
 * Usage:
 *   wl_set_callback(wl_recv_callback, nullptr);
 *   wl_init(port, local_player_id);
 *
 *   GekkoNetAdapter adapter = wl_gekko_adapter();
 *   gekko_net_adapter_set(session, &adapter);
 *
 *   // Register remote peers — address is a single byte: the WL player ID.
 *   uint8_t remote_id = 2;
 *   GekkoNetAddress addr = { &remote_id, sizeof(remote_id) };
 *   gekko_add_actor(session, GekkoRemotePlayer, &addr);
 *
 * GekkoNet calls receive_data() each gekko_network_poll() to drain accumulated
 * packets, then calls free_data() on each result when it is done with it.
 * send_data() is called per outbound message.
 *
 * The WL callback runs on a background IO thread; the mutex protects the queue.
 * receive_data() is always called from the game thread (single-threaded
 * contract).
 */

#include <cstdlib>
#include <cstring>
#include <mutex>
#include <queue>
#include <vector>

#include "../wl-sdk.h"
#include "gekkonet.h"

/* ── Types and state ────────────────────────────────────────────────────────
 */

/* Layout of each allocation — built once in the callback, handed straight to
 * GekkoNet. GekkoNet receives &result->net_result as GekkoNetResult* (first
 * field = valid cast). free_data() frees the whole block with one free(). */
struct WlResult {
  GekkoNetResult net_result;
  uint8_t        from_id;
  uint8_t        payload[]; /* data lives here — one copy, no intermediate */
};

static std::mutex              g_mutex;
static std::queue<WlResult *>  g_queue;
static std::vector<GekkoNetResult *> g_results; /* persistent; avoids realloc between polls */

/* ── Callback ────────────────────────────────────────────────────────────────
 */

/* WL SDK callback — fires on the background IO thread.
 * Builds the final WlResult in place; no second copy needed in receive_data. */
void wl_recv_callback(uint8_t from, const void *data, int len, void * /*userdata*/) {
  if (len <= 0)
    return;

  WlResult *result = (WlResult *)malloc(sizeof(WlResult) + len);
  result->from_id = from;
  memcpy(result->payload, data, len);
  result->net_result.addr     = {&result->from_id, 1};
  result->net_result.data     = result->payload;
  result->net_result.data_len = (unsigned int)len;
  std::lock_guard<std::mutex> lock(g_mutex);
  g_queue.push(result);
}

/* ── Adapter functions ───────────────────────────────────────────────────────
 */

static void adapter_send(GekkoNetAddress *addr, const char *data, int length) {
  if (!addr || !addr->data || addr->size < 1)
    return;

  uint8_t id = *(uint8_t *)addr->data;
  wl_send(id, data, length);
}

static GekkoNetResult **adapter_receive(int *out_count) {
  std::queue<WlResult *> local;
  {
    std::lock_guard<std::mutex> lock(g_mutex);
    std::swap(local, g_queue);
  }

  g_results.clear();
  while (!local.empty()) {
    g_results.push_back(&local.front()->net_result);
    local.pop();
  }

  *out_count = (int)g_results.size();
  return g_results.empty() ? nullptr : g_results.data();
}

static void adapter_free(void *ptr) {
  free(ptr); /* single block allocated in wl_recv_callback */
}

/* ── Public ──────────────────────────────────────────────────────────────────
 */

GekkoNetAdapter wl_gekko_adapter() {
  return GekkoNetAdapter{adapter_send, adapter_receive, adapter_free};
}
