#pragma once

// C ABI over the Weyvelength client, for FFI from any language. Mirrors the
// C++ Weyvelength::Client one-to-one; see client.h for the semantics of each
// call. All strings are UTF-8 byte ranges, never assumed null-free.

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#else
#include <stdbool.h>
#endif

// Static by default. Define WEYVE_DLL to build or consume this as a shared
// library, and WEYVE_DLL_EXPORT additionally when building the DLL itself.
#if defined(_WIN32) && defined(WEYVE_DLL)
#ifdef WEYVE_DLL_EXPORT
#define WEYVE_API __declspec(dllexport)
#else
#define WEYVE_API __declspec(dllimport)
#endif
#else
#define WEYVE_API
#endif

typedef struct WeyveClient WeyveClient; // opaque; owns one server connection and its p2p mesh

// Mirrors Proto::RoomErrorCode; carried by WEYVE_EVENT_ROOM_ERROR.
typedef enum WeyveRoomError {
	WEYVE_ROOM_ERROR_ALREADY_IN_ROOM,
	WEYVE_ROOM_ERROR_NO_SUCH_ROOM,
	WEYVE_ROOM_ERROR_NOT_IN_ROOM,
	WEYVE_ROOM_ERROR_NOT_HOST, // host-only action attempted by a non-host
	WEYVE_ROOM_ERROR_BAD_ROOM_DATA, // key/value over the size limits, or too many keys
	WEYVE_ROOM_ERROR_NO_SUCH_MEMBER, // target id is not another member of the room
	WEYVE_ROOM_ERROR_ROOM_CLOSED, // the room is not joinable right now
	WEYVE_ROOM_ERROR_BAD_PASSWORD, // wrong password on join, or an over-long one on set
	WEYVE_ROOM_ERROR_BANNED, // the host has barred this client from the room
} WeyveRoomError;

// The kinds of event weyve_next can hand back. These are exactly the server
// messages the client surfaces; id/ice/signaling frames are consumed inside.
typedef enum WeyveEventType {
	WEYVE_EVENT_NONE = 0,
	WEYVE_EVENT_HEARTBEAT, // pong; timestamp echoes the one you sent
	WEYVE_EVENT_ROOM_ID_ASSIGNED, // create/join succeeded
	WEYVE_EVENT_ROOM_ERROR, // a room request failed
	WEYVE_EVENT_CHAT, // someone broadcast text to the room
	WEYVE_EVENT_PEER_JOINED, // another client is in the room
	WEYVE_EVENT_PEER_LEFT, // a client left; from == weyve_id confirms your own leave
	WEYVE_EVENT_HOST_CHANGED, // the room's current host
	WEYVE_EVENT_ROOM_DATA_CHANGED, // one room metadata key; empty value means deleted
	WEYVE_EVENT_MEMBER_DATA_CHANGED, // one member metadata key; empty value means deleted
	WEYVE_EVENT_KICKED, // the host removed you from the room
	WEYVE_EVENT_BANNED, // the host removed you and barred you from rejoining
	WEYVE_EVENT_ROOM_ACCESS_CHANGED, // the room's joinability/password flag changed
} WeyveEventType;

// One decoded server message. String pointers borrow client-owned storage and
// stay valid only until the next weyve_next or weyve_poll on the same client.
typedef struct WeyveEvent {
	WeyveEventType type;
	union {
		struct { uint64_t timestamp; } heartbeat; // WEYVE_EVENT_HEARTBEAT
		struct { const char* id; uint32_t id_len; } room_assigned; // WEYVE_EVENT_ROOM_ID_ASSIGNED
		struct { WeyveRoomError code; const char* context; uint32_t context_len; } room_error; // WEYVE_EVENT_ROOM_ERROR
		struct { uint32_t from; const char* text; uint32_t text_len; } chat; // WEYVE_EVENT_CHAT
		struct { uint32_t id; } peer_joined; // WEYVE_EVENT_PEER_JOINED
		struct { uint32_t id; } peer_left; // WEYVE_EVENT_PEER_LEFT
		struct { uint32_t id; } host_changed; // WEYVE_EVENT_HOST_CHANGED
		struct { const char* key; uint32_t key_len; const char* value; uint32_t value_len; } room_data; // WEYVE_EVENT_ROOM_DATA_CHANGED
		struct { uint32_t id; const char* key; uint32_t key_len; const char* value; uint32_t value_len; } member_data; // WEYVE_EVENT_MEMBER_DATA_CHANGED
		struct { bool open; bool passworded; } room_access; // WEYVE_EVENT_ROOM_ACCESS_CHANGED
	} data;
} WeyveEvent;

// --- lifecycle ---

WEYVE_API WeyveClient* weyve_client_create(void); // never null; pair with weyve_client_destroy
WEYVE_API void weyve_client_destroy(WeyveClient* client); // null is a no-op

WEYVE_API bool weyve_connect(WeyveClient* client, const char* host, uint16_t port);
WEYVE_API bool weyve_poll(WeyveClient* client); // instant, non-blocking; false once the connection is gone
WEYVE_API bool weyve_next(WeyveClient* client, WeyveEvent* out); // one queued event per call; false when drained

WEYVE_API bool weyve_send_heartbeat(WeyveClient* client, uint64_t timestamp); // server pongs it back

// --- rooms ---

WEYVE_API bool weyve_create_room(WeyveClient* client); // -> WEYVE_EVENT_ROOM_ID_ASSIGNED or WEYVE_EVENT_ROOM_ERROR
WEYVE_API bool weyve_join_room(WeyveClient* client, const char* id, const char* password); // password may be null or ""
WEYVE_API bool weyve_leave_room(WeyveClient* client); // -> WEYVE_EVENT_PEER_LEFT carrying your own id

WEYVE_API bool weyve_kick_member(WeyveClient* client, uint32_t id); // host-only
WEYVE_API bool weyve_ban_member(WeyveClient* client, uint32_t id); // host-only
WEYVE_API bool weyve_transfer_host(WeyveClient* client, uint32_t id); // host-only

WEYVE_API bool weyve_set_room_joinable(WeyveClient* client, bool open); // host-only
WEYVE_API bool weyve_set_room_password(WeyveClient* client, const char* password); // host-only; null or "" clears it

WEYVE_API bool weyve_send_chat(WeyveClient* client, const char* text);
WEYVE_API bool weyve_set_room_data(WeyveClient* client, const char* key, const char* value); // host-only
WEYVE_API bool weyve_delete_room_data(WeyveClient* client, const char* key); // host-only
WEYVE_API bool weyve_set_member_data(WeyveClient* client, const char* key, const char* value); // your own slots
WEYVE_API bool weyve_delete_member_data(WeyveClient* client, const char* key);

// --- peer to peer ---

WEYVE_API bool weyve_send_p2p(WeyveClient* client, uint32_t id, const void* data, uint32_t len); // first send builds the link
// One received datagram; returns its bytes or null when none are queued. The
// buffer borrows client-owned storage, valid until the next weyve_next_p2p call.
WEYVE_API const uint8_t* weyve_next_p2p(WeyveClient* client, uint32_t* from, uint32_t* len);
WEYVE_API bool weyve_peer_connected(WeyveClient* client, uint32_t id); // is a direct link up right now?

// --- cached room state ---
// Getters below read the client's cache; nothing blocks or hits the network.

WEYVE_API uint32_t weyve_id(const WeyveClient* client); // 0 until the server assigns one
WEYVE_API uint32_t weyve_host_id(const WeyveClient* client); // 0 until a room is joined
WEYVE_API bool weyve_is_host(const WeyveClient* client);
WEYVE_API bool weyve_room_joinable(const WeyveClient* client);
WEYVE_API bool weyve_room_passworded(const WeyveClient* client); // the flag only; the password never reaches clients

// Current room id as a byte range; len 0 means not in a room. Borrowed, valid until the next weyve_poll.
WEYVE_API const char* weyve_room_id(const WeyveClient* client, uint32_t* len);

// Room members, ourselves included. Borrowed, valid until the next weyve_poll.
WEYVE_API const uint32_t* weyve_members(const WeyveClient* client, uint32_t* count);

// Room metadata: look one key up, or walk the keys by index. Values are byte
// ranges; a null return means the key is unset. Borrowed, valid until the next weyve_poll.
WEYVE_API const char* weyve_room_data(const WeyveClient* client, const char* key, uint32_t* value_len);
WEYVE_API uint32_t weyve_room_data_count(const WeyveClient* client);
WEYVE_API const char* weyve_room_data_key_at(const WeyveClient* client, uint32_t index, uint32_t* key_len);

// Member metadata, same shape, scoped to one member id.
WEYVE_API const char* weyve_member_data(const WeyveClient* client, uint32_t id, const char* key, uint32_t* value_len);
WEYVE_API uint32_t weyve_member_data_count(const WeyveClient* client, uint32_t id);
WEYVE_API const char* weyve_member_data_key_at(const WeyveClient* client, uint32_t id, uint32_t index, uint32_t* key_len);

#ifdef __cplusplus
}
#endif
