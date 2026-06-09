#pragma once

#ifdef _WIN32
#define _WIN32_WINNT 0x0A00
#endif // _WIN32

#include <chrono>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <memory>
#include <unordered_map>
#include <vector>

#include <thirdparty\asio\asio.hpp>

#include "protocol.h"

namespace Weyvelength {

	struct ServerConfig {
		uint16_t port = 0;
	};

	// shared_ptr-owned: the session coroutine keeps it alive, while the id map
	// lets other code (future signaling) find it by id.
	struct Connection {
		uint32_t id = 0;
		asio::ip::tcp::socket socket;

		std::deque<std::vector<std::byte>> out;   // outbound queue; WriteLoop is the sole writer
		asio::steady_timer wake;                   // cancel() signals "out has work"
		bool closing = false;

		Connection(uint32_t id, asio::ip::tcp::socket socket)
			: id(id), socket(std::move(socket)), wake(this->socket.get_executor()) {
			wake.expires_at(std::chrono::steady_clock::time_point::max());
		}
	};

	struct Server {
		bool Init(ServerConfig& config);   // open/bind/listen; false on bind error
		void Run();                        // spawns the accept loop, then blocks
		void Stop();                       // stops the io_context

		void SendTo(uint32_t id, const Proto::ServerMessage& msg);
		void SendToMany(const std::vector<uint32_t>& ids, const Proto::ServerMessage& msg);

	private:
		asio::awaitable<void> AcceptLoop();
		asio::awaitable<void> Session(std::shared_ptr<Connection> conn);
		asio::awaitable<void> ReadLoop(std::shared_ptr<Connection> conn);
		asio::awaitable<void> WriteLoop(std::shared_ptr<Connection> conn);

		void SendFrame(uint32_t id, std::vector<std::byte> frame);

		asio::io_context _context;
		asio::ip::tcp::acceptor _acceptor{ _context };

		// Keyed by id so a connection can be found for signaling later.
		std::unordered_map<uint32_t, std::shared_ptr<Connection>> _connections;
		uint32_t _next_id = 1;   // 0 reserved as "none"
		ServerConfig _config;
	};
}
