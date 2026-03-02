/*
 * wl-sdk.h — Weyvelength SDK public API
 *
 * Call wl_init() once at startup, then wl_poll() each tick to drain inbound
 * packets. Use wl_send() to transmit data to a peer by player ID.
 *
 * All functions must be called from a single thread.
 * Link against wl-sdk.lib (Windows) or libwl-sdk.a (POSIX).
 */

#pragma once

#ifdef __cplusplus
extern "C"
{
#endif

#include <stdint.h>

#ifdef WL_DLL
#ifdef WL_BUILDING
#define WL_API __declspec(dllexport)
#else
#define WL_API __declspec(dllimport)
#endif
#else
#define WL_API
#endif

#define WL_PACKET_MAX 1400

    typedef enum WL_EventKind
    {
        WL_EVENT_NONE = 0,
        WL_EVENT_PACKET = 1, /* data received from a peer */
    } WL_EventKind;

    /* Pointer valid until the next wl_poll(). Do not free. */
    typedef struct WL_Event
    {
        WL_EventKind kind;
        uint8_t from_player_id;
        int data_len; /* bytes valid in data[] */
        uint8_t data[WL_PACKET_MAX];
    } WL_Event;

    /*
     * Initialise the SDK. Pass --wl-udp-port as bridge_port and --wl-player-id
     * as local_player_id directly from argv. Returns 0 on success.
     */
    WL_API int wl_init(int bridge_port, uint8_t local_player_id);

    /* Release all resources. Safe to call before wl_init() or after failure. */
    WL_API void wl_shutdown(void);

    /* Last error string, valid until the next SDK call. */
    WL_API const char *wl_last_error(void);

    /* This player's ID from the config (0 = Spectator/Inactive or not yet init'd). */
    WL_API uint8_t wl_local_player_id(void);

    /*
     * Drain all packets received since the last call. Returns a pointer to an
     * internal event array and writes the count to *out_count. The pointer is
     * valid until the next wl_poll(). Returns NULL when there are no events.
     *
     *   int n; const WL_Event* ev = wl_poll(&n);
     *   for (int i = 0; i < n; ++i) handle(ev[i]);
     */
    WL_API const WL_Event *wl_poll(int *out_count);

    /*
     * Send data_len bytes to the peer with to_player_id.
     * Returns 0 on success, non-zero on error.
     */
    WL_API int wl_send(uint8_t to_player_id, const void *data, int data_len);

#ifdef __cplusplus
}
#endif
