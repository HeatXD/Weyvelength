#include <chrono>
#include <iostream>
#include <mutex>
#include <queue>
#include <string>
#include <thread>

#include "weyvelength.h"

// Typed lines cross from the blocking console reader to the poll loop here.
static std::mutex input_mutex;
static std::queue<std::string> input_lines;

static void StartInputThread()
{
	std::thread([] {
		std::string line;
		while (std::getline(std::cin, line)) {
			std::lock_guard lock(input_mutex);
			input_lines.push(std::move(line));
		}
	}).detach();
}

static bool NextInputLine(std::string& out)
{
	std::lock_guard lock(input_mutex);
	if (input_lines.empty())
		return false;

	out = std::move(input_lines.front());
	input_lines.pop();
	return true;
}

static void PrintPong(const Weyvelength::Client& client, const Weyvelength::Proto::Heartbeat& pong)
{
	uint64_t now_ticks = std::chrono::steady_clock::now().time_since_epoch().count();
	uint64_t rtt_ticks = now_ticks - pong.timestamp;

	auto rtt = std::chrono::steady_clock::duration(rtt_ticks);
	auto rtt_ms = std::chrono::duration_cast<std::chrono::milliseconds>(rtt).count();

	std::cout << "Pong: " << rtt_ms << " ms (i am client " << client.Id() << ")\n";
}

static void PrintRoomError(const Weyvelength::Proto::RoomError& error)
{
	std::cout << "Room error " << (int)error.code;
	if (!error.context.empty())
		std::cout << " (" << error.context << ")";
	std::cout << "\n";
}

// Ping the server once a second; it replies with a pong.
static int RunPing(Weyvelength::Client& client)
{
	using namespace Weyvelength;

	auto last = std::chrono::steady_clock::now();
	while (client.Poll()) {
		Proto::ServerMessage msg;
		while (client.Next(msg)) {
			if (auto* pong = std::get_if<Proto::Heartbeat>(&msg))
				PrintPong(client, *pong);
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

// Create a room (empty code) or join one, then send typed lines to everyone in it.
static int RunChat(Weyvelength::Client& client, const std::string& code)
{
	using namespace Weyvelength;

	if (code.empty())
		client.CreateRoom();
	else
		client.JoinRoom(code);

	StartInputThread();

	while (client.Poll()) {
		Proto::ServerMessage msg;
		while (client.Next(msg)) {
			if (auto* room = std::get_if<Proto::AssignRoomId>(&msg)) {
				std::cout << "In room " << room->id << " (join it: test chat " << room->id << ")\n";
			}
			else if (auto* error = std::get_if<Proto::RoomError>(&msg)) {
				PrintRoomError(*error);
				return 1;
			}
			else if (auto* chat = std::get_if<Proto::RoomChat>(&msg)) {
				std::cout << "[client " << chat->from << "] " << chat->text << "\n";
			}
		}

		// Lines typed before the room is joined stay queued until it is.
		std::string line;
		while (!client.RoomId().empty() && NextInputLine(line)) {
			if (!line.empty())
				client.SendChat(line);
		}

		std::this_thread::sleep_for(std::chrono::milliseconds(1));
	}

	std::cout << "Connection closed\n";
	return 0;
}

int main(int argc, char* argv[])
{
	using namespace Weyvelength;

	std::cout << std::unitbuf; // flush every print so redirected output is live

	std::string mode = argc > 1 ? argv[1] : "";
	if (mode != "ping" && mode != "chat") {
		std::cout << "usage: test ping        heartbeat/rtt demo\n";
		std::cout << "       test chat        create a room and chat in it\n";
		std::cout << "       test chat CODE   join a room and chat in it\n";
		return 1;
	}

	ClientConfig config{ .host = "127.0.0.1", .port = 5555 };

	Client client;
	if (!client.Connect(config)) {
		std::cout << "Connect to " << config.host << ":" << config.port << " failed\n";
		return 1;
	}

	std::cout << "Connected to " << config.host << ":" << config.port << "\n";

	if (mode == "ping")
		return RunPing(client);
	return RunChat(client, argc > 2 ? argv[2] : "");
}
