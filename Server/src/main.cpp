#include <cstdlib>
#include <iostream>
#include <string>

#include <spdlog/async.h>
#include <spdlog/sinks/stdout_color_sinks.h>
#include <spdlog/spdlog.h>

#include "weyvelength_server.h"

using namespace Weyvelength;

static void PrintUsage()
{
	std::cout <<
		"usage: weyvelength_server [options]\n"
		"  --port PORT                  default 5555\n"
		"  --room-code-length N         default 8\n"
		"  --room-list-cooldown-ms MS   default 1000\n"
		"  --stun-host HOST             default stun.l.google.com; empty disables it\n"
		"  --stun-port PORT             default 19302\n"
		"  --turn HOST:PORT:USER:PASS   repeatable\n";
}

// TURN servers arrive as one "host:port:user:pass" argument per --turn flag.
static bool ParseTurnServer(const std::string& arg, Proto::TurnServer& out)
{
	size_t a = arg.find(':');
	size_t b = arg.find(':', a == std::string::npos ? a : a + 1);
	size_t c = arg.find(':', b == std::string::npos ? b : b + 1);
	if (a == std::string::npos || b == std::string::npos || c == std::string::npos)
		return false;

	out.host = arg.substr(0, a);
	out.port = (uint16_t)std::strtoul(arg.substr(a + 1, b - a - 1).c_str(), nullptr, 10);
	out.username = arg.substr(b + 1, c - b - 1);
	out.password = arg.substr(c + 1);
	return true;
}

static bool ParseArgs(int argc, char* argv[], ServerConfig& config)
{
	for (int i = 1; i < argc; i++) {
		std::string arg = argv[i];
		auto next = [&]() -> const char* { return i + 1 < argc ? argv[++i] : nullptr; };

		if (arg == "--help" || arg == "-h") {
			PrintUsage();
			std::exit(0);
		} else if (arg == "--port") {
			config.port = (uint16_t)std::strtoul(next(), nullptr, 10);
		} else if (arg == "--room-code-length") {
			config.room_code_length = (uint32_t)std::strtoul(next(), nullptr, 10);
		} else if (arg == "--room-list-cooldown-ms") {
			config.room_list_cooldown_ms = (uint32_t)std::strtoul(next(), nullptr, 10);
		} else if (arg == "--stun-host") {
			config.ice.stun_host = next();
		} else if (arg == "--stun-port") {
			config.ice.stun_port = (uint16_t)std::strtoul(next(), nullptr, 10);
		} else if (arg == "--turn") {
			Proto::TurnServer turn;
			if (!ParseTurnServer(next(), turn)) {
				std::cout << "bad --turn value, expected HOST:PORT:USER:PASS\n";
				return false;
			}
			config.ice.turn.push_back(std::move(turn));
		} else {
			std::cout << "unknown option: " << arg << "\n";
			return false;
		}
	}
	return true;
}

int main(int argc, char* argv[])
{
	ServerConfig config{ .port = 5555, .ice = { .stun_host = "stun.l.google.com", .stun_port = 19302 } };
	if (!ParseArgs(argc, argv, config)) {
		PrintUsage();
		return 1;
	}

	// The library logs through the default logger; the app decides what that
	// is. Here: async so logging never blocks the io thread, flushed per
	// message so redirected output stays live.
	spdlog::init_thread_pool(8192, 1);
	spdlog::set_default_logger(spdlog::create_async<spdlog::sinks::stdout_color_sink_mt>("weyvelength"));
	spdlog::set_level(spdlog::level::debug); // dev server: show the p2p signal traffic
	spdlog::flush_on(spdlog::level::debug);

	Server server;
	if (!server.Init(config)) {
		spdlog::error("Server failed to bind port {}", config.port);
		return 1;
	}

	server.Run();   // blocks, driving the accept/ping/read coroutines
	return 0;
}
