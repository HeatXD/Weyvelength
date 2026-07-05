#include "weyvelength_server.h"

#include <array>
#include <random>
#include <string>
#include <utility>
#include <vector>

#include <spdlog/spdlog.h>
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

	static std::string MakeRoomCode(uint32_t length)
	{
		static std::mt19937 rng{ std::random_device{}() };
		static constexpr char alphabet[] = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no 0/O/1/I
		std::uniform_int_distribution<size_t> pick{ 0, sizeof(alphabet) - 2 };

		std::string code(length, '?');
		for (char& c : code)
			c = alphabet[pick(rng)];
		return code;
	}

	bool Server::Init(ServerConfig& config)
	{
		_config = config;
		if (_config.room_code_length == 0)
			_config.room_code_length = 8;

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

		spdlog::info("Listening on port {}", config.port);
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

			spdlog::info("Client {} connected", id);
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

		spdlog::info("Client {} disconnected", conn->id);
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
		else if (std::get_if<Proto::LeaveRoom>(&msg)) {
			HandleLeaveRoom(conn);
		}
		else if (auto* chat = std::get_if<Proto::RoomChat>(&msg)) {
			HandleRoomChat(conn, *chat);
		}
		else if (auto* set = std::get_if<Proto::SetRoomData>(&msg)) {
			HandleSetRoomData(conn, *set);
		}
		else if (auto* setMember = std::get_if<Proto::SetMemberData>(&msg)) {
			HandleSetMemberData(conn, *setMember);
		}
	}

	void Server::HandleCreateRoom(const std::shared_ptr<Connection>& conn)
	{
		if (!conn->room.empty()) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::AlreadyInRoom, conn->room });
			return;
		}

		std::string code = MakeRoomCode(_config.room_code_length);
		while (_rooms.contains(code))
			code = MakeRoomCode(_config.room_code_length);

		_rooms.emplace(code, Room{ code, conn->id, { conn->id } });
		conn->room = code;
		SendTo(conn->id, Proto::AssignRoomId{ code });
		SendTo(conn->id, Proto::HostChanged{ conn->id }); // the host cache has a single source: this event

		spdlog::info("Client {} created room {}", conn->id, code);
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

		Room& room = it->second;
		SendToMany(room.members, Proto::PeerJoined{ conn->id });

		// hydrate the joiner with the same events everyone else already
		// understands: one per existing member, the host, one per data key
		SendTo(conn->id, Proto::AssignRoomId{ msg.id });

		for (uint32_t member : room.members) {
			SendTo(conn->id, Proto::PeerJoined{ member });
		}

		SendTo(conn->id, Proto::HostChanged{ room.host });

		for (const auto& [key, value] : room.data) {
			SendTo(conn->id, Proto::RoomDataChanged{ key, value });
		}

		for (const auto& [member, data] : room.member_data) {
			for (const auto& [key, value] : data) {
				SendTo(conn->id, Proto::MemberDataChanged{ member, key, value });
			}
		}

		room.members.push_back(conn->id);
		conn->room = msg.id;

		spdlog::info("Client {} joined room {}", conn->id, msg.id);
	}

	void Server::HandleLeaveRoom(const std::shared_ptr<Connection>& conn)
	{
		if (conn->room.empty()) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::NotInRoom, {} });
			return;
		}

		SendTo(conn->id, Proto::PeerLeft{ conn->id }); // your own id = you left
		LeaveRoom(conn);
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

	void Server::HandleSetRoomData(const std::shared_ptr<Connection>& conn, const Proto::SetRoomData& msg)
	{
		auto it = _rooms.find(conn->room);
		if (it == _rooms.end()) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::NotInRoom, {} });
			return;
		}

		Room& room = it->second;
		if (room.host != conn->id) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::NotHost, msg.key });
			return;
		}

		if (msg.key.empty() || msg.key.size() > Proto::max_room_data_key || msg.value.size() > Proto::max_room_data_value) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::BadRoomData, msg.key });
			return;
		}

		if (msg.value.empty()) { // empty value = delete
			if (room.data.erase(msg.key) == 0)
				return; // nothing deleted, nothing to announce
		}
		else {
			auto [entry, inserted] = room.data.try_emplace(msg.key, msg.value);
			if (inserted && room.data.size() > Proto::max_room_data_keys) {
				room.data.erase(entry);
				SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::BadRoomData, msg.key });
				return;
			}
			if (!inserted) {
				if (entry->second == msg.value)
					return; // unchanged, nothing to announce
				entry->second = msg.value;
			}
		}

		// the setter applies the write when the broadcast comes back, like
		// everyone else, so all members see changes in the same order
		SendToMany(room.members, Proto::RoomDataChanged{ msg.key, msg.value });
	}

	void Server::HandleSetMemberData(const std::shared_ptr<Connection>& conn, const Proto::SetMemberData& msg)
	{
		auto it = _rooms.find(conn->room);
		if (it == _rooms.end()) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::NotInRoom, {} });
			return;
		}

		if (msg.key.empty() || msg.key.size() > Proto::max_room_data_key || msg.value.size() > Proto::max_room_data_value) {
			SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::BadRoomData, msg.key });
			return;
		}

		Room& room = it->second;
		if (msg.value.empty()) { // empty value = delete
			auto member = room.member_data.find(conn->id);
			if (member == room.member_data.end() || member->second.erase(msg.key) == 0)
				return; // nothing deleted, nothing to announce
			if (member->second.empty())
				room.member_data.erase(member);
		}
		else {
			auto& data = room.member_data[conn->id];
			auto [entry, inserted] = data.try_emplace(msg.key, msg.value);
			if (inserted && data.size() > Proto::max_member_data_keys) {
				data.erase(entry);
				SendTo(conn->id, Proto::RoomError{ Proto::RoomErrorCode::BadRoomData, msg.key });
				return;
			}
			if (!inserted) {
				if (entry->second == msg.value)
					return; // unchanged, nothing to announce
				entry->second = msg.value;
			}
		}

		SendToMany(room.members, Proto::MemberDataChanged{ conn->id, msg.key, msg.value });
	}

	void Server::LeaveRoom(const std::shared_ptr<Connection>& conn)
	{
		if (conn->room.empty())
			return;

		auto it = _rooms.find(conn->room);
		if (it != _rooms.end()) {
			Room& room = it->second;
			std::erase(room.members, conn->id);
			room.member_data.erase(conn->id);
			if (room.members.empty()) {
				_rooms.erase(it);
				spdlog::info("Room {} closed", conn->room);
			}
			else {
				SendToMany(room.members, Proto::PeerLeft{ conn->id });
				if (room.host == conn->id) {
					room.host = room.members.front(); // oldest remaining member
					SendToMany(room.members, Proto::HostChanged{ room.host });
					spdlog::info("Client {} now hosts room {}", room.host, conn->room);
				}
			}
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
