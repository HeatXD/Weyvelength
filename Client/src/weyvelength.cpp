#include "weyvelength.h"

#ifdef _WIN32
#define _WIN32_WINNT 0x0A00
#endif // _WIN32

#include <array>
#include <span>
#include <vector>

#include <thirdparty\asio\asio.hpp>
#include <thirdparty\zpp_bits\zpp_bits.h>

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

	bool Client::SendChat(const std::string& text)
	{
		return SendServer(Proto::RoomChat{ 0, text });   // server fills in the sender id
	}

	uint32_t Client::Id() const
	{
		return _id;
	}

	const std::string& Client::RoomId() const
	{
		return _room;
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
				if (auto* room = std::get_if<Proto::AssignRoomId>(&msg))
					_room = room->id;   // cached for RoomId(), but still surfaced via Next()
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

	bool Client::DisconnectServer()
	{
		_asio->socket.close();
		return false;
	}
}
