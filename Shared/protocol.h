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
	};

	struct CreateRoom {}; // client -> server: create a room and join it
	struct JoinRoom { std::string id; }; // client -> server: join an existing room by id

	struct RoomError { // server -> client: a room request failed
		RoomErrorCode code{};
		std::string context; // extra detail, e.g. the offending room id
	};

	struct RoomChat { // client -> server: broadcast text to the sender's room
		uint32_t from = 0; // server -> client: filled in with the sender's id
		std::string text;
	};

	// All traffic on the server connection, both directions.
	using ServerMessage = std::variant<Heartbeat, AssignClientId, AssignRoomId, CreateRoom, JoinRoom, RoomError, RoomChat>;

	struct tmp {};
	using P2PMessage = std::variant<tmp>;   // peer-to-peer channel, unused for now

	constexpr uint32_t max_message_size = 1024;
}
