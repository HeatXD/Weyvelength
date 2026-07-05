#pragma once

#include <cstdint>
#include <string>
#include <variant>

namespace Weyvelength::Proto {
	struct Heartbeat { uint64_t timestamp; };   // server <-> client heartbeat
	struct AssignClientId { uint32_t id = 0; };  // server -> client: the client's own connection id
	struct AssignRoomId { std::string id; }; // server -> client: the room id the client has joined successfully

	enum class RoomErrorCode : uint8_t {
		AlreadyInRoom,
		NoSuchRoom,
		NotInRoom,
		NotHost, // room data writes are host-only
		BadRoomData, // key/value over the size limits, or too many keys
	};

	struct CreateRoom {}; // client -> server: create a room and join it
	struct JoinRoom { std::string id; }; // client -> server: join an existing room by id
	struct LeaveRoom {}; // client -> server: leave the current room

	struct RoomError { // server -> client: a room request failed
		RoomErrorCode code{};
		std::string context; // extra detail, e.g. the offending room id
	};

	struct RoomChat { // client -> server: broadcast text to the sender's room
		uint32_t from = 0; // server -> client: filled in with the sender's id
		std::string text;
	};

	struct PeerJoined { uint32_t id = 0; }; // server -> client: another client is in the room (live join, or replayed to a joiner per existing member)
	struct PeerLeft { uint32_t id = 0; }; // server -> client: a client left the room; your own id confirms your LeaveRoom
	struct HostChanged { uint32_t id = 0; }; // server -> client: the room's current host

	struct SetRoomData { // client -> server: set one room metadata key; empty value deletes it
		std::string key;
		std::string value;
	};

	struct RoomDataChanged { // server -> client: one room metadata key changed; empty value means deleted
		std::string key;
		std::string value;
	};

	struct SetMemberData { // client -> server: set one key of your own member metadata; empty value deletes it
		std::string key;
		std::string value;
	};

	struct MemberDataChanged { // server -> client: one key of a member's metadata changed; empty value means deleted
		uint32_t id = 0; // whose data
		std::string key;
		std::string value;
	};

	// All traffic on the server connection, both directions. Only append new
	// messages: zpp_bits encodes the variant index, so inserting in the middle
	// breaks peers built against the old order.
	using ServerMessage = std::variant<Heartbeat, AssignClientId, AssignRoomId, CreateRoom, JoinRoom, RoomError, RoomChat,
		LeaveRoom, PeerJoined, PeerLeft, HostChanged, SetRoomData, RoomDataChanged, SetMemberData, MemberDataChanged>;

	struct tmp {};
	using P2PMessage = std::variant<tmp>;   // peer-to-peer channel, unused for now

	constexpr uint32_t max_message_size = 1024;

	// Metadata limits, shared by room and member data. One key/value pair per
	// frame, so the pair caps keep every data message under max_message_size.
	constexpr uint32_t max_room_data_key = 128;
	constexpr uint32_t max_room_data_value = 512;
	constexpr uint32_t max_room_data_keys = 64;
	constexpr uint32_t max_member_data_keys = 16;
}
