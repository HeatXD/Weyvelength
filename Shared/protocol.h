#pragma once

#include <cstdint>
#include <variant>

namespace Weyvelength::Proto {
	struct Ping {};   // server -> client heartbeat
	struct Pong {};   // client -> server reply
	struct AssignId { uint32_t id = 0; };   // server -> client: the client's own connection id

	// All traffic on the server connection, both directions.
	using ServerMessage = std::variant<Ping, Pong, AssignId>;

	struct tmp {};
	using P2PMessage = std::variant<tmp>;   // peer-to-peer channel, unused for now

	constexpr uint32_t max_message_size = 1024;
}
