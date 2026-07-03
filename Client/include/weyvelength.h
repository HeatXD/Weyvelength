#pragma once

#include <cstdint>
#include <memory>
#include <queue>
#include <string>

#include "protocol.h"

namespace Weyvelength {
	struct ClientAsioImpl;

	struct ClientConfig {
		std::string host = "127.0.0.1";
		uint16_t    port = 0;
	};

	struct Client {
		Client();
		~Client();

		bool Connect(ClientConfig& config);

		bool Poll();

		bool Next(Proto::ServerMessage& out);

		bool SendServer(const Proto::ServerMessage& msg);

		bool CreateRoom();                      // server replies AssignRoomId or RoomError
		bool JoinRoom(const std::string& id);   // server replies AssignRoomId or RoomError
		bool SendChat(const std::string& text); // broadcast to everyone in the current room

		uint32_t Id() const;  // 0 until the server has assigned one
		const std::string& RoomId() const;  // empty until a room has been joined

	private:
		bool PollServer();
		bool DrainServer();
		bool CarveServer();
		bool FlushServer();
		bool DisconnectServer();

		std::unique_ptr<ClientAsioImpl> _asio;
		std::queue<Proto::ServerMessage> _inbox;
		uint32_t _id = 0;
		std::string _room;
	};
}
