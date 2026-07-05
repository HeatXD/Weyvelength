#include "weyvelength.h"

#ifdef _WIN32
#define _WIN32_WINNT 0x0A00
#endif // _WIN32

#include <array>
#include <span>
#include <vector>

#include <thirdparty/asio/asio.hpp>
#include <thirdparty/zpp_bits/zpp_bits.h>

#include "framing.h"

namespace Weyvelength {
	struct ClientAsioImpl {
		asio::io_context context;
		asio::ip::tcp::socket socket{ context };
		std::vector<std::byte> rx;   // bytes received but not yet consumed
		std::vector<std::byte> tx;   // framed bytes queued to send
	};

	Client::Client() : _asio(std::make_unique<ClientAsioImpl>()) {}
	Client::~Client() = default;

	bool Client::Connect(ClientConfig& config)
	{
		auto& impl = *_asio;
		asio::error_code ec;

		asio::ip::tcp::resolver resolver{ impl.context };
		auto endpoints = resolver.resolve(config.host, std::to_string(config.port), ec);
		if (ec) return false;

		asio::connect(impl.socket, endpoints, ec);
		if (ec) return false;

		impl.socket.non_blocking(true, ec);
		return !ec;
	}

	bool Client::Poll()
	{
		return PollServer();
	}

	bool Client::PollServer()
	{
		if (!_asio->socket.is_open())
			return false;

		return DrainServer() && CarveServer() && FlushServer();
	}

	bool Client::Next(Proto::ServerMessage& out)
	{
		if (_inbox.empty())
			return false;

		out = std::move(_inbox.front());
		_inbox.pop();
		return true;
	}

	bool Client::SendServer(const Proto::ServerMessage& msg)
	{
		auto frame = Proto::FrameMessage(msg);
		_asio->tx.insert(_asio->tx.end(), frame.begin(), frame.end());
		return true;
	}

	bool Client::CreateRoom()
	{
		return SendServer(Proto::CreateRoom{});
	}

	bool Client::JoinRoom(const std::string& id)
	{
		return SendServer(Proto::JoinRoom{ id });
	}

	bool Client::LeaveRoom()
	{
		return SendServer(Proto::LeaveRoom{});
	}

	bool Client::SendChat(const std::string& text)
	{
		return SendServer(Proto::RoomChat{ 0, text }); // server fills in the sender id
	}

	bool Client::SetRoomData(const std::string& key, const std::string& value)
	{
		return SendServer(Proto::SetRoomData{ key, value });
	}

	bool Client::DeleteRoomData(const std::string& key)
	{
		return SendServer(Proto::SetRoomData{ key, {} }); // empty value = delete
	}

	bool Client::SetMemberData(const std::string& key, const std::string& value)
	{
		return SendServer(Proto::SetMemberData{ key, value });
	}

	bool Client::DeleteMemberData(const std::string& key)
	{
		return SendServer(Proto::SetMemberData{ key, {} }); // empty value = delete
	}

	uint32_t Client::Id() const
	{
		return _id;
	}

	const std::string& Client::RoomId() const
	{
		return _room;
	}

	uint32_t Client::Host() const
	{
		return _host;
	}

	bool Client::IsHost() const
	{
		return _id != 0 && _id == _host;
	}

	const std::vector<uint32_t>& Client::Members() const
	{
		return _members;
	}

	const std::map<std::string, std::string>& Client::RoomData() const
	{
		return _data;
	}

	const std::string* Client::RoomData(const std::string& key) const
	{
		auto it = _data.find(key);
		return it == _data.end() ? nullptr : &it->second;
	}

	const std::map<std::string, std::string>* Client::MemberData(uint32_t id) const
	{
		auto it = _member_data.find(id);
		return it == _member_data.end() ? nullptr : &it->second;
	}

	const std::string* Client::MemberData(uint32_t id, const std::string& key) const
	{
		const auto* data = MemberData(id);
		if (!data)
			return nullptr;

		auto it = data->find(key);
		return it == data->end() ? nullptr : &it->second;
	}

	bool Client::DrainServer()
	{
		auto& impl = *_asio;

		std::array<std::byte, 4096> chunk;
		asio::error_code ec;

		while (true) {
			size_t chunk_len = impl.socket.read_some(asio::buffer(chunk), ec);
			if (ec) break;
			impl.rx.insert(impl.rx.end(), chunk.begin(), chunk.begin() + chunk_len);
		}

		if (ec != asio::error::would_block && ec != asio::error::try_again) {
			return DisconnectServer();   // eof or hard error
		}

		return true;
	}

	bool Client::CarveServer()
	{
		auto& impl = *_asio;
		constexpr size_t header_size = sizeof(uint32_t);

		std::span<std::byte> remaining{ impl.rx };
		while (remaining.size() >= header_size) {
			uint32_t len;
			if (failure(zpp::bits::in{ remaining.first(header_size) }(len)))
				return DisconnectServer();
			if (len > Proto::max_message_size)
				return DisconnectServer();
			if (remaining.size() < header_size + len)
				break;   // full frame hasn't arrived yet

			Proto::ServerMessage msg;
			zpp::bits::in body{ remaining.subspan(header_size, len) };
			if (failure(body(msg)))
				return DisconnectServer();

			if (auto* assign = std::get_if<Proto::AssignClientId>(&msg)) {
				_id = assign->id;   // transport metadata; not surfaced via Next()
			}
			else {
				CacheRoomState(msg); // cached for the accessors, but still surfaced via Next()
				_inbox.push(std::move(msg));
			}
			remaining = remaining.subspan(header_size + len);
		}

		size_t consumed = impl.rx.size() - remaining.size();
		impl.rx.erase(impl.rx.begin(), impl.rx.begin() + consumed);
		return true;
	}

	bool Client::FlushServer()
	{
		auto& impl = *_asio;
		asio::error_code ec;

		while (!impl.tx.empty()) {
			size_t sent = impl.socket.write_some(asio::buffer(impl.tx), ec);
			if (ec) break;
			impl.tx.erase(impl.tx.begin(), impl.tx.begin() + sent);
		}

		if (ec && ec != asio::error::would_block && ec != asio::error::try_again) {
			return DisconnectServer();   // hard error (empty tx leaves ec unset)
		}

		return true;
	}

	// Mirrors room events into the local caches as frames are carved, so the
	// accessors are already current when Next() hands the event to the app.
	void Client::CacheRoomState(const Proto::ServerMessage& msg)
	{
		if (auto* room = std::get_if<Proto::AssignRoomId>(&msg)) {
			_room = room->id;
			_host = 0;
			_members.assign(1, _id); // events only ever announce the others
			_data.clear();
			_member_data.clear();
		}
		else if (auto* joined = std::get_if<Proto::PeerJoined>(&msg)) {
			_members.push_back(joined->id);
		}
		else if (auto* left = std::get_if<Proto::PeerLeft>(&msg)) {
			if (left->id == _id) {
				ClearRoomState(); // our own id = our LeaveRoom went through
			}
			else {
				std::erase(_members, left->id);
				_member_data.erase(left->id);
			}
		}
		else if (auto* host = std::get_if<Proto::HostChanged>(&msg)) {
			_host = host->id;
		}
		else if (auto* data = std::get_if<Proto::RoomDataChanged>(&msg)) {
			if (data->value.empty())
				_data.erase(data->key);
			else
				_data[data->key] = data->value;
		}
		else if (auto* member = std::get_if<Proto::MemberDataChanged>(&msg)) {
			if (member->value.empty()) {
				auto it = _member_data.find(member->id);
				if (it != _member_data.end()) {
					it->second.erase(member->key);
					if (it->second.empty())
						_member_data.erase(it);
				}
			}
			else {
				_member_data[member->id][member->key] = member->value;
			}
		}
	}

	void Client::ClearRoomState()
	{
		_room.clear();
		_host = 0;
		_members.clear();
		_data.clear();
		_member_data.clear();
	}

	bool Client::DisconnectServer()
	{
		_asio->socket.close();
		ClearRoomState();
		return false;
	}
}
