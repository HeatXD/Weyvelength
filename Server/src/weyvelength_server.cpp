#include "weyvelength_server.h"

#include <array>
#include <iostream>
#include <utility>
#include <vector>

#include <thirdparty\asio\asio.hpp>
#include <thirdparty\zpp_bits\zpp_bits.h>

#include "framing.h"

using asio::use_awaitable;

namespace Weyvelength {

	static void Enqueue(const std::shared_ptr<Connection>& conn, std::vector<std::byte> frame)
	{
		conn->out.push_back(std::move(frame));
		conn->wake.cancel();
	}

	bool Server::Init(ServerConfig& config)
	{
		_config = config;
		asio::error_code ec;

		asio::ip::tcp::endpoint endpoint{ asio::ip::tcp::v4(), config.port };

		_acceptor.open(endpoint.protocol(), ec);
		if (ec) return false;

		_acceptor.set_option(asio::socket_base::reuse_address(true), ec);
		if (ec) return false;

		_acceptor.bind(endpoint, ec);
		if (ec) return false;

		_acceptor.listen(asio::socket_base::max_listen_connections, ec);
		if (ec) return false;

		std::cout << "Listening on port " << config.port << "\n";
		return true;
	}

	void Server::Run()
	{
		asio::co_spawn(_context, AcceptLoop(), asio::detached);
		_context.run();
	}

	void Server::Stop()
	{
		_context.stop();
	}

	asio::awaitable<void> Server::AcceptLoop()
	{
		while (true) {
			asio::ip::tcp::socket socket = co_await _acceptor.async_accept(use_awaitable);

			uint32_t id = _next_id++;   // single-threaded io_context: no lock needed
			auto conn = std::make_shared<Connection>(id, std::move(socket));
			_connections.emplace(id, conn);

			std::cout << "Client " << id << " connected\n";
			asio::co_spawn(_context, Session(conn), asio::detached);
		}
	}

	asio::awaitable<void> Server::Session(std::shared_ptr<Connection> conn)
	{
		asio::co_spawn(conn->socket.get_executor(), WriteLoop(conn), asio::detached);

		SendTo(conn->id, Proto::AssignId{ conn->id });

		try {
			co_await ReadLoop(conn);
		}
		catch (...) {
		}

		conn->closing = true;
		conn->wake.cancel();

		asio::error_code ec;

		conn->socket.close(ec);
		_connections.erase(conn->id);

		std::cout << "Client " << conn->id << " disconnected\n";
	}

	asio::awaitable<void> Server::ReadLoop(std::shared_ptr<Connection> conn)
	{
		while (true) {
			std::array<std::byte, Proto::frame_header_size> header;
			co_await asio::async_read(conn->socket, asio::buffer(header), use_awaitable);

			uint32_t len;
			if (!Proto::DecodeFrameLength(header, len))
				co_return;   // protocol error? end the session

			std::vector<std::byte> body(len);
			co_await asio::async_read(conn->socket, asio::buffer(body), use_awaitable);

			Proto::ServerMessage msg;
			if (failure(zpp::bits::in{ body }(msg)))
				co_return;

			if (std::holds_alternative<Proto::Ping>(msg))
				SendTo(conn->id, Proto::Pong{});
		}
	}

	asio::awaitable<void> Server::WriteLoop(std::shared_ptr<Connection> conn)
	{
		try {
			while (!conn->closing) {
				if (conn->out.empty()) {
					asio::error_code ec;
					co_await conn->wake.async_wait(asio::redirect_error(use_awaitable, ec));
					continue;
				}

				std::vector<std::byte> frame = std::move(conn->out.front());
				conn->out.pop_front();
				co_await asio::async_write(conn->socket, asio::buffer(frame), use_awaitable);
			}
		}
		catch (...) {
		}
	}

	void Server::SendFrame(uint32_t id, std::vector<std::byte> frame)
	{
		auto it = _connections.find(id);
		if (it != _connections.end())
			Enqueue(it->second, std::move(frame));
	}

	void Server::SendTo(uint32_t id, const Proto::ServerMessage& msg)
	{
		SendFrame(id, Proto::FrameMessage(msg));
	}

	void Server::SendToMany(const std::vector<uint32_t>& ids, const Proto::ServerMessage& msg)
	{
		std::vector<std::byte> frame = Proto::FrameMessage(msg);   // serialize once

		for (uint32_t id : ids)
			SendFrame(id, frame);
	}
}
