#pragma once

#ifdef _WIN32
#define _WIN32_WINNT 0x0A00
#endif // _WIN32

#include <chrono>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

#include <thirdparty\asio\asio.hpp>

#include "protocol.h"

namespace Weyvelength {

	struct ServerConfig {
		uint16_t port = 0;
		uint32_t room_code_length = 0; // 0 = use the default (8)
	};

	struct Connection {
		uint32_t id = 0;
		std::string room; // empty = not in a room
		asio::ip::tcp::socket socket;

		std::deque<std::vector<std::byte>> out; // outbound queue; WriteLoop is the sole writer
		asio::steady_timer wake; // cancel() signals "out has work"
		bool closing = false;

		Connection(uint32_t id, asio::ip::tcp::socket socket)
			: id(id), socket(std::move(socket)), wake(this->socket.get_executor()) {
			// maybe this isnt the smartest thing to do but its ok for now
			wake.expires_at(std::chrono::steady_clock::time_point::max());
		}
	};

	struct Room {
		std::string id;
		std::vector<uint32_t> members;
	};

	struct Server {
		bool Init(ServerConfig& config);
		void Run();
		void Stop();

	private:
		void SendTo(uint32_t id, const Proto::ServerMessage& msg);
		void SendToMany(const std::vector<uint32_t>& ids, const Proto::ServerMessage& msg);
		void SendFrame(uint32_t id, std::vector<std::byte> frame);
		void HandleMessage(std::shared_ptr<Connection> conn, const Proto::ServerMessage& msg);
		void HandleCreateRoom(const std::shared_ptr<Connection>& conn);
		void HandleJoinRoom(const std::shared_ptr<Connection>& conn, const Proto::JoinRoom& msg);
		void HandleRoomChat(const std::shared_ptr<Connection>& conn, const Proto::RoomChat& msg);
		void LeaveRoom(const std::shared_ptr<Connection>& conn);

		asio::awaitable<void> AcceptLoop();
		asio::awaitable<void> Session(std::shared_ptr<Connection> conn);
		asio::awaitable<void> ReadLoop(std::shared_ptr<Connection> conn);
		asio::awaitable<void> WriteLoop(std::shared_ptr<Connection> conn);

		asio::io_context _context;
		asio::ip::tcp::acceptor _acceptor{ _context };

		std::unordered_map<uint32_t, std::shared_ptr<Connection>> _connections;
		std::unordered_map<std::string, Room> _rooms;
		uint32_t _next_id = 1;   // 0 reserved as "none"
		ServerConfig _config;
	};
}
