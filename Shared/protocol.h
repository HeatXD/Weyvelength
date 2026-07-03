#pragma once

#include <cstdint>
#include <variant>

namespace Weyvelength::Proto {
	struct Heartbeat { uint64_t timestamp; };   // server <-> client heartbeat
	struct AssignClientId { uint32_t id = 0; };  // server -> client: the client's own connection id
	struct AssignRoomId { std::string id; }; // server -> client: the room id the client has joined successfully

	// All traffic on the server connection, both directions.
	using ServerMessage = std::variant<Heartbeat, AssignClientId, AssignRoomId>;

	struct tmp {};
	using P2PMessage = std::variant<tmp>;   // peer-to-peer channel, unused for now

	constexpr uint32_t max_message_size = 1024;
}
