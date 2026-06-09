#include <chrono>
#include <iostream>
#include <thread>

#include "weyvelength.h"

int main()
{
	using namespace Weyvelength;

	ClientConfig config{ .host = "127.0.0.1", .port = 5555 };

	Client client;
	if (!client.Connect(config)) {
		std::cout << "Connect to " << config.host << ":" << config.port << " failed\n";
		return 1;
	}

	std::cout << "Connected to " << config.host << ":" << config.port << "\n";

	// Ping the server once a second; it replies with a pong.
	auto last = std::chrono::steady_clock::now();
	while (client.Poll()) {
		Proto::ServerMessage msg;
		while (client.Next(msg)) {
			if (std::holds_alternative<Proto::Pong>(msg))
				std::cout << "Pong (i am client " << client.Id() << ")\n";
		}

		auto now = std::chrono::steady_clock::now();
		if (now - last >= std::chrono::seconds(1)) {
			client.Send(Proto::Ping{});
			last = now;
		}
		std::this_thread::sleep_for(std::chrono::milliseconds(10));
	}

	std::cout << "Connection closed\n";
	return 0;
}
