#include <spdlog/async.h>
#include <spdlog/sinks/stdout_color_sinks.h>
#include <spdlog/spdlog.h>

#include "weyvelength_server.h"

int main()
{
	using namespace Weyvelength;

	// The library logs through the default logger; the app decides what that
	// is. Here: async so logging never blocks the io thread, flushed per
	// message so redirected output stays live.
	spdlog::init_thread_pool(8192, 1);
	spdlog::set_default_logger(spdlog::create_async<spdlog::sinks::stdout_color_sink_mt>("weyvelength"));
	spdlog::set_level(spdlog::level::debug); // dev server: show the p2p signal traffic
	spdlog::flush_on(spdlog::level::debug);

	ServerConfig config{ .port = 5555, .ice = { .stun_host = "stun.l.google.com", .stun_port = 19302 } };

	Server server;
	if (!server.Init(config)) {
		spdlog::error("Server failed to bind port {}", config.port);
		return 1;
	}

	server.Run();   // blocks, driving the accept/ping/read coroutines
	return 0;
}
