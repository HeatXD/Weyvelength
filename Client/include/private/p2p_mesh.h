#pragma once

#include <cstddef>
#include <cstdint>
#include <deque>
#include <map>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

#include <juice/juice.h>

// Client-internal state behind the p2p mesh; keeps juice types out of the API.
namespace Weyvelength {
	struct JuiceEvent { // one callback crossing from juice's threads into Poll
		enum class Kind { State, Candidate, GatheringDone, Recv };
		Kind kind{};
		uint32_t peer = 0;
		juice_state_t state = JUICE_STATE_DISCONNECTED; // Kind::State only
		std::vector<std::byte> payload; // candidate sdp or Kind::Recv datagram
	};

	struct JuiceCallbackContext { // one agent's user_ptr; freed after juice_destroy
		struct P2PMesh* mesh = nullptr;
		uint32_t peer = 0;
	};

	struct PeerLink { // one lazily built direct connection
		juice_agent_t* agent = nullptr;
		std::unique_ptr<JuiceCallbackContext> ctx;
		bool remote_set = false; // juice_set_remote_description may only run once
		bool connected = false;
		std::deque<std::vector<std::byte>> outbox; // datagrams queued until the link connects
	};

	struct P2PMesh {
		std::mutex mutex; // guards events only; links belong to the poll thread
		std::vector<JuiceEvent> events; // juice callbacks push, PollPeers drains
		std::vector<JuiceEvent> scratch; // the drained batch; reused so capacity sticks
		std::map<uint32_t, PeerLink> links;
		std::map<uint32_t, uint32_t> attempts; // per-peer ICE tries; outlives links to cap retries
	};
}
