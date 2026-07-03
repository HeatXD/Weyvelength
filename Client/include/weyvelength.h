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

		uint32_t Id() const;  // 0 until the server has assigned one

	private:
		bool PollServer();
		bool DrainServer();
		bool CarveServer();
		bool FlushServer();
		bool DisconnectServer();

		std::unique_ptr<ClientAsioImpl> _asio;
		std::queue<Proto::ServerMessage> _inbox;
		uint32_t _id = 0;
	};
}
