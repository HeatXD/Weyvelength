#include "weyvelength_server.h"

#include <array>
#include <iostream>
#include <random>
#include <string>
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

	static std::string MakeRoomCode()
	{
		static std::mt19937 rng{ std::random_device{}() };
		static constexpr char alphabet[] = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";   // no 0/O/1/I
		std::uniform_int_distribution<size_t> pick{ 0, sizeof(alphabet) - 2 };

		std::string code(5, '?');
		for (char& c : code)
			c = alphabet[pick(rng)];
		return code;
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

		SendTo(conn->id, Proto::AssignClientId{ conn->id });

		try {
			co_await ReadLoop(conn);
		}
		catch (...) {
		}

		conn->closing = true;
		conn->wake.cancel();

		asio::error_code ec;

		conn->socket.close(ec);
		LeaveRoom(conn);
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

			HandleMessage(conn, msg);
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
		if (it != _connections.end()) {
			Enqueue(it->second, std::move(frame));
		}
	}

	void Server::HandleMessage(std::shared_ptr<Connection> conn, const Proto::ServerMessage& msg)
	{
		if (auto* ping = std::get_if<Proto::Heartbeat>(&msg)) {
			SendTo(conn->id, Proto::Heartbeat{ ping->timestamp });
		}
		else if (std::get_if<Proto::CreateRoom>(&msg)) {
			HandleCreateRoom(conn);
		}
		else if (auto* join = std::get_if<Proto::JoinRoom>(&msg)) {
			HandleJoinRoom(conn, *join);
		}
		else if (auto* chat = std::get_if<Proto::RoomChat>(&msg)) {
			HandleRoomChat(conn, *chat);
		}
	}

	void Server::HandleCreateRoom(const std::shared_ptr<Connection>& conn)
	{
		if (!conn->room.empty()) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::AlreadyInRoom, conn->room });
			return;
		}

		std::string code = MakeRoomCode();
		while (_rooms.contains(code))
			code = MakeRoomCode();

		_rooms.emplace(code, Room{ code, { conn->id } });
		conn->room = code;
		SendTo(conn->id, Proto::AssignRoomId{ code });

		std::cout << "Client " << conn->id << " created room " << code << "\n";
	}

	void Server::HandleJoinRoom(const std::shared_ptr<Connection>& conn, const Proto::JoinRoom& msg)
	{
		if (!conn->room.empty()) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::AlreadyInRoom, conn->room });
			return;
		}

		auto it = _rooms.find(msg.id);
		if (it == _rooms.end()) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::NoSuchRoom, msg.id });
			return;
		}

		it->second.members.push_back(conn->id);
		conn->room = msg.id;
		SendTo(conn->id, Proto::AssignRoomId{ msg.id });

		std::cout << "Client " << conn->id << " joined room " << msg.id << "\n";
	}

	void Server::HandleRoomChat(const std::shared_ptr<Connection>& conn, const Proto::RoomChat& msg)
	{
		auto it = _rooms.find(conn->room);
		if (it == _rooms.end()) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::NotInRoom, {} });
			return;
		}

		// sender included: everyone in the room sees the same stream
		SendToMany(it->second.members, Proto::RoomChat{ conn->id, msg.text });
	}

	void Server::LeaveRoom(const std::shared_ptr<Connection>& conn)
	{
		if (conn->room.empty())
			return;

		auto it = _rooms.find(conn->room);
		if (it != _rooms.end()) {
			std::erase(it->second.members, conn->id);
			if (it->second.members.empty())
				_rooms.erase(it);
		}
		conn->room.clear();
	}

	void Server::SendTo(uint32_t id, const Proto::ServerMessage& msg)
	{
		SendFrame(id, Proto::FrameMessage(msg));
	}

	void Server::SendToMany(const std::vector<uint32_t>& ids, const Proto::ServerMessage& msg)
	{
		std::vector<std::byte> frame = Proto::FrameMessage(msg);   // serialize once

		for (uint32_t id : ids) {
			SendFrame(id, frame);
		}
	}
}
