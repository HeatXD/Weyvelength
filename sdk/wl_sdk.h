/*
 * wl-sdk.h, Weyvelength SDK public API
 *
 * 1. Register a callback with wl_set_callback().
 * 2. Call wl_init() with the args passed by the Weyvelength client.
 * 3. Call wl_send() to transmit. The callback fires on the SDK's background
 *    IO thread whenever a packet arrives. `data` is valid only for the
 *    duration of the callback — copy it if you need it afterwards.
 * 4. Call wl_shutdown() on exit.
 *
 * wl_send() is the only function safe to call from the callback thread.
 * All other functions must be called from a single thread.
 * Link against wl_sdk.dll.lib (Windows) or libwl_sdk.so (Linux).
 */

#pragma once

#ifdef __cplusplus
extern "C"
{
#endif

#include <stdint.h>

#ifdef _WIN32
#define WL_API __declspec(dllimport)
#else
#define WL_API
#endif

    /*
     * Fired on the SDK's background IO thread the instant a packet arrives.
     * `data` points into the SDK's receive buffer — valid only during this call.
     * wl_send() is safe to call from inside the callback.
     */
    typedef void (*WL_RecvCallback)(
        uint8_t from_player_id,
        const void *data,
        int data_len,
        void *userdata);

    /* Register the callback. Call before wl_init(). Pass NULL to unregister. */
    WL_API void wl_set_callback(WL_RecvCallback cb, void *userdata);

    /* Initialise. Pass --wl-udp-port and --wl-player-id from argv. Returns 0 on success. */
    WL_API int wl_init(int bridge_port, uint8_t local_player_id);

    /* Release all resources. */
    WL_API void wl_shutdown(void);

    /* Last error string, valid until the next SDK call. */
    WL_API const char *wl_last_error(void);

    /* This player's ID (0 = Spectator/Inactive or not yet init'd). */
    WL_API uint8_t wl_local_player_id(void);

    /* Send data_len bytes to the peer with to_player_id. Returns 0 on success. */
    WL_API int wl_send(uint8_t to_player_id, const void *data, int data_len);

#ifdef __cplusplus
}
#endif
