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
			if (auto* pong = std::get_if<Proto::Heartbeat>(&msg)) {
				uint64_t now_ticks = std::chrono::steady_clock::now().time_since_epoch().count();
				uint64_t rtt_ticks = now_ticks - pong->timestamp;

				auto rtt = std::chrono::steady_clock::duration(rtt_ticks);
				auto rtt_ms = std::chrono::duration_cast<std::chrono::milliseconds>(rtt).count();

				std::cout << "Pong (i am client " << client.Id() << ")\n";
				std::cout << "RTT: " << rtt_ms << " ms\n";
			}
		}

		auto now = std::chrono::steady_clock::now();
		if (now - last >= std::chrono::seconds(1)) {
			client.SendServer(Proto::Heartbeat{ (uint64_t)now.time_since_epoch().count() });
			last = now;
		}
		std::this_thread::sleep_for(std::chrono::milliseconds(1));
	}

	std::cout << "Connection closed\n";
	return 0;
}
