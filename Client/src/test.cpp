#include <chrono>
#include <cstdlib>
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

// "/kick 3" style numeric argument; 0 (never a valid id) on garbage.
static uint32_t ParseId(const std::string& arg)
{
	return (uint32_t)std::strtoul(arg.c_str(), nullptr, 10);
}

static void PrintRoomInfo(const Weyvelength::Client& client)
{
	std::cout << "Room " << client.RoomId() << ", host " << client.HostId() << (client.IsHost() ? " (you)" : "") << "\n";
	std::cout << (client.RoomJoinable() ? "Open to join" : "Closed") << (client.RoomPassworded() ? ", password required" : "") << "\n";

	std::cout << "Members:";
	for (uint32_t id : client.Members()) {
		std::cout << " " << id;
	}
	std::cout << "\n";

	for (const auto& [key, value] : client.RoomData()) {
		std::cout << "  " << key << " = " << value << "\n";
	}

	for (uint32_t id : client.Members()) {
		const auto* data = client.MemberData(id);
		if (!data)
			continue;

		for (const auto& [key, value] : *data) {
			std::cout << "  client " << id << ": " << key << " = " << value << "\n";
		}
	}
}

// "/p2p 3 hello"; sends the text bytes to one peer over the mesh.
static void SendP2PCommand(Weyvelength::Client& client, const std::string& args)
{
	size_t space = args.find(' ');
	uint32_t id = ParseId(args.substr(0, space));
	if (space == std::string::npos || id == 0) {
		std::cout << "usage: /p2p ID TEXT\n";
		return;
	}

	std::string text = args.substr(space + 1);
	auto* bytes = (const std::byte*)text.data();
	if (!client.SendP2P(id, { bytes, bytes + text.size() }))
		std::cout << "p2p send to client " << id << " failed\n";
}

// "/set KEY VALUE" or "/setme KEY VALUE"; the value may contain spaces.
static void SendSetCommand(Weyvelength::Client& client, const std::string& args, bool own)
{
	size_t space = args.find(' ');
	if (space == std::string::npos || space == 0) {
		std::cout << "usage: " << (own ? "/setme" : "/set") << " KEY VALUE\n";
		return;
	}

	std::string key = args.substr(0, space);
	std::string value = args.substr(space + 1);
	if (own)
		client.SetMemberData(key, value);
	else
		client.SetRoomData(key, value);
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
static int RunChat(Weyvelength::Client& client, const std::string& code, const std::string& password)
{
	using namespace Weyvelength;

	if (code.empty())
		client.CreateRoom();
	else
		client.JoinRoom(code, password);

	StartInputThread();

	while (client.Poll()) {
		Proto::ServerMessage msg;
		while (client.Next(msg)) {
			if (auto* room = std::get_if<Proto::AssignRoomId>(&msg)) {
				std::cout << "In room " << room->id << " (join it: test chat " << room->id << ")\n";
				std::cout << "Commands: /who, /set KEY VALUE, /del KEY, /setme KEY VALUE, /delme KEY\n";
				std::cout << "          /open, /close, /pass [PASSWORD], /kick ID, /ban ID, /host ID, /leave\n";
				std::cout << "          /p2p ID TEXT (direct, over the mesh)\n";
			}
			else if (auto* error = std::get_if<Proto::RoomError>(&msg)) {
				PrintRoomError(*error);
				if (client.RoomId().empty())
					return 1; // create/join failed; nothing to chat in
			}
			else if (auto* chat = std::get_if<Proto::RoomChat>(&msg)) {
				std::cout << "[client " << chat->from << "] " << chat->text << "\n";
			}
			else if (auto* joined = std::get_if<Proto::PeerJoined>(&msg)) {
				std::cout << "* client " << joined->id << " is here\n";
			}
			else if (auto* left = std::get_if<Proto::PeerLeft>(&msg)) {
				if (left->id == client.Id()) {
					std::cout << "Left the room\n";
					return 0;
				}
				std::cout << "* client " << left->id << " left\n";
			}
			else if (auto* host = std::get_if<Proto::HostChanged>(&msg)) {
				std::cout << "* client " << host->id << " is the host" << (host->id == client.Id() ? " (you)" : "") << "\n";
			}
			else if (auto* data = std::get_if<Proto::RoomDataChanged>(&msg)) {
				if (data->value.empty())
					std::cout << "* room data: " << data->key << " deleted\n";
				else
					std::cout << "* room data: " << data->key << " = " << data->value << "\n";
			}
			else if (auto* member = std::get_if<Proto::MemberDataChanged>(&msg)) {
				if (member->value.empty())
					std::cout << "* client " << member->id << " data: " << member->key << " deleted\n";
				else
					std::cout << "* client " << member->id << " data: " << member->key << " = " << member->value << "\n";
			}
			else if (std::get_if<Proto::KickedByHost>(&msg)) {
				std::cout << "Kicked from the room\n";
				return 0;
			}
			else if (std::get_if<Proto::BannedByHost>(&msg)) {
				std::cout << "Banned from the room\n";
				return 0;
			}
			else if (auto* access = std::get_if<Proto::RoomAccessChanged>(&msg)) {
				std::cout << "* room is now " << (access->open ? "open" : "closed") << (access->passworded ? " (password required)" : "") << "\n";
			}
		}

		uint32_t from = 0;
		Proto::P2PMessage data;
		while (client.NextP2P(from, data)) {
			std::string text{ (const char*)data.data(), data.size() };
			std::cout << "[p2p client " << from << "] " << text << "\n";
		}

		// Lines typed before the room is joined stay queued until it is.
		std::string line;
		while (!client.RoomId().empty() && NextInputLine(line)) {
			if (line.empty())
				continue;
			if (line == "/who")
				PrintRoomInfo(client);
			else if (line == "/leave")
				client.LeaveRoom();
			else if (line.rfind("/setme ", 0) == 0)
				SendSetCommand(client, line.substr(7), true);
			else if (line.rfind("/delme ", 0) == 0)
				client.DeleteMemberData(line.substr(7));
			else if (line.rfind("/set ", 0) == 0)
				SendSetCommand(client, line.substr(5), false);
			else if (line.rfind("/del ", 0) == 0)
				client.DeleteRoomData(line.substr(5));
			else if (line == "/open")
				client.SetRoomJoinable(true);
			else if (line == "/close")
				client.SetRoomJoinable(false);
			else if (line == "/pass")
				client.SetRoomPassword({});
			else if (line.rfind("/pass ", 0) == 0)
				client.SetRoomPassword(line.substr(6));
			else if (line.rfind("/kick ", 0) == 0)
				client.KickMember(ParseId(line.substr(6)));
			else if (line.rfind("/ban ", 0) == 0)
				client.BanMember(ParseId(line.substr(5)));
			else if (line.rfind("/host ", 0) == 0)
				client.TransferHost(ParseId(line.substr(6)));
			else if (line.rfind("/p2p ", 0) == 0)
				SendP2PCommand(client, line.substr(5));
			else
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
		std::cout << "       test chat CODE [PASSWORD]   join a room and chat in it\n";
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
	return RunChat(client, argc > 2 ? argv[2] : "", argc > 3 ? argv[3] : "");
}
