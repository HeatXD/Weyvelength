#include <chrono>
#include <iostream>
#include <string>
#include <thread>

#include "weyvelength.h"

int main(int argc, char* argv[])
{
	using namespace Weyvelength;

	std::cout << std::unitbuf;   // flush every print so redirected output is live

	ClientConfig config{ .host = "127.0.0.1", .port = 5555 };

	Client client;
	if (!client.Connect(config)) {
		std::cout << "Connect to " << config.host << ":" << config.port << " failed\n";
		return 1;
	}

	std::cout << "Connected to " << config.host << ":" << config.port << "\n";

	// No argument: create a room. Argument: join that room by code.
	if (argc > 1)
		client.JoinRoom(argv[1]);
	else
		client.CreateRoom();

	// Ping the server every 5 seconds; chat into the room every 2 seconds.
	auto last_ping = std::chrono::steady_clock::now();
	auto last_chat = last_ping;
	int chat_count = 0;

	while (client.Poll()) {
		Proto::ServerMessage msg;
		while (client.Next(msg)) {
			if (auto* pong = std::get_if<Proto::Heartbeat>(&msg)) {
				uint64_t now_ticks = std::chrono::steady_clock::now().time_since_epoch().count();
				uint64_t rtt_ticks = now_ticks - pong->timestamp;

				auto rtt = std::chrono::steady_clock::duration(rtt_ticks);
				auto rtt_ms = std::chrono::duration_cast<std::chrono::milliseconds>(rtt).count();

				std::cout << "Pong: " << rtt_ms << " ms (i am client " << client.Id() << ")\n";
			}
			else if (auto* room = std::get_if<Proto::AssignRoomId>(&msg)) {
				std::cout << "In room " << room->id << " (join it: test " << room->id << ")\n";
			}
			else if (auto* error = std::get_if<Proto::RoomError>(&msg)) {
				std::cout << "Room error " << (int)error->code;
				if (!error->context.empty())
					std::cout << " (" << error->context << ")";
				std::cout << "\n";
				return 1;
			}
			else if (auto* chat = std::get_if<Proto::RoomChat>(&msg)) {
				std::cout << "[client " << chat->from << "] " << chat->text << "\n";
			}
		}

		auto now = std::chrono::steady_clock::now();
		if (now - last_ping >= std::chrono::seconds(5)) {
			client.SendServer(Proto::Heartbeat{ (uint64_t)now.time_since_epoch().count() });
			last_ping = now;
		}
		if (!client.RoomId().empty() && now - last_chat >= std::chrono::seconds(2)) {
			client.SendChat("hello #" + std::to_string(++chat_count));
			last_chat = now;
		}
		std::this_thread::sleep_for(std::chrono::milliseconds(1));
	}

	std::cout << "Connection closed\n";
	return 0;
}
