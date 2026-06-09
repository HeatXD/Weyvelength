#include <iostream>

#include "weyvelength_server.h"

int main()
{
	using namespace Weyvelength;

	ServerConfig config{ .port = 5555 };

	Server server;
	if (!server.Init(config)) {
		std::cout << "Server failed to bind port " << config.port << "\n";
		return 1;
	}

	server.Run();   // blocks, driving the accept/ping/read coroutines
	return 0;
}
