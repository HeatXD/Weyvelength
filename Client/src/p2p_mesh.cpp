#include "weyvelength.h"
#include "private/p2p_mesh.h"

#include <algorithm>

// The p2p half of the client: a mesh of libjuice links that builds itself
// lazily. The first SendP2P to a peer starts ICE, signaled through the server
// as P2PSignal frames; the peer answers the same way when the signals arrive.
namespace Weyvelength {

	// The four juice callbacks run on juice's own threads; they only queue
	// events, everything real happens on the poll thread.
	static void PushJuiceEvent(void* user_ptr, JuiceEvent ev)
	{
		auto* ctx = static_cast<JuiceCallbackContext*>(user_ptr);
		ev.peer = ctx->peer;

		std::lock_guard lock(ctx->mesh->mutex);
		ctx->mesh->events.push_back(std::move(ev));
	}

	static void OnJuiceState(juice_agent_t*, juice_state_t state, void* user_ptr)
	{
		PushJuiceEvent(user_ptr, { .kind = JuiceEvent::Kind::State, .state = state });
	}

	static void OnJuiceCandidate(juice_agent_t*, const char* sdp, void* user_ptr)
	{
		PushJuiceEvent(user_ptr, { .kind = JuiceEvent::Kind::Candidate, .payload = sdp });
	}

	static void OnJuiceGatheringDone(juice_agent_t*, void* user_ptr)
	{
		PushJuiceEvent(user_ptr, { .kind = JuiceEvent::Kind::GatheringDone });
	}

	static void OnJuiceRecv(juice_agent_t*, const char* data, size_t size, void* user_ptr)
	{
		PushJuiceEvent(user_ptr, { .kind = JuiceEvent::Kind::Recv, .payload = { data, size } });
	}

	bool Client::SendP2P(uint32_t id, const Proto::P2PMessage& msg)
	{
		if (msg.empty() || msg.size() > Proto::max_p2p_message_size)
			return false;

		if (id == _id || std::ranges::find(_members, id) == _members.end())
			return false;

		PeerLink* link = FindLink(id);
		if (!link) {
			link = CreateLink(id); // lazy: the first message to a peer starts ICE
			if (!link)
				return false;
			if (!ShareLink(*link, id)) {
				DestroyLink(id);
				return false;
			}
		}

		if (!link->connected) {
			link->outbox.push_back(msg); // flushed when the link comes up
			return true;
		}

		return juice_send(link->agent, reinterpret_cast<const char*>(msg.data()), msg.size()) == JUICE_ERR_SUCCESS;
	}

	bool Client::NextP2P(uint32_t& from, Proto::P2PMessage& out)
	{
		if (_p2p_inbox.empty())
			return false;

		from = _p2p_inbox.front().first;
		out = std::move(_p2p_inbox.front().second);
		_p2p_inbox.pop();
		return true;
	}

	bool Client::PeerConnected(uint32_t id) const
	{
		auto it = _mesh->links.find(id);
		return it != _mesh->links.end() && it->second.connected;
	}

	PeerLink* Client::FindLink(uint32_t id)
	{
		auto it = _mesh->links.find(id);
		return it == _mesh->links.end() ? nullptr : &it->second;
	}

	PeerLink* Client::CreateLink(uint32_t id)
	{
		PeerLink link;
		link.ctx = std::make_unique<JuiceCallbackContext>();
		link.ctx->mesh = _mesh.get();
		link.ctx->peer = id;

		std::vector<juice_turn_server_t> turn;
		for (const Proto::TurnServer& relay : _ice.turn) {
			turn.push_back({ relay.host.c_str(), relay.username.c_str(), relay.password.c_str(), relay.port });
		}

		juice_config_t config{};
		config.concurrency_mode = JUICE_CONCURRENCY_MODE_POLL; // every link shares one juice thread
		if (!_ice.stun_host.empty()) {
			config.stun_server_host = _ice.stun_host.c_str();
			config.stun_server_port = _ice.stun_port;
		}
		config.turn_servers = turn.empty() ? nullptr : turn.data();
		config.turn_servers_count = (int)turn.size();
		config.cb_state_changed = OnJuiceState;
		config.cb_candidate = OnJuiceCandidate;
		config.cb_gathering_done = OnJuiceGatheringDone;
		config.cb_recv = OnJuiceRecv;
		config.user_ptr = link.ctx.get();

		link.agent = juice_create(&config);
		if (!link.agent)
			return nullptr;

		return &_mesh->links.emplace(id, std::move(link)).first->second;
	}

	// Announces a fresh agent to its peer: the ufrag/pwd description first,
	// then gathering trickles candidates through OnJuiceCandidate.
	bool Client::ShareLink(PeerLink& link, uint32_t id)
	{
		char sdp[JUICE_MAX_SDP_STRING_LEN];
		if (juice_get_local_description(link.agent, sdp, sizeof(sdp)) != JUICE_ERR_SUCCESS)
			return false;

		SendServer(Proto::P2PSignal{ id, Proto::P2PSignalKind::Description, sdp });
		return juice_gather_candidates(link.agent) == JUICE_ERR_SUCCESS;
	}

	// Sends everything that queued up while the link was still connecting.
	void Client::FlushLink(PeerLink& link)
	{
		for (const std::vector<std::byte>& data : link.outbox) {
			juice_send(link.agent, reinterpret_cast<const char*>(data.data()), data.size());
		}
		link.outbox.clear();
	}

	void Client::DestroyLink(uint32_t id)
	{
		auto it = _mesh->links.find(id);
		if (it == _mesh->links.end())
			return;

		juice_destroy(it->second.agent); // returns once the agent's callbacks have
		_mesh->links.erase(it); // so the callback context is safe to free here
	}

	void Client::DestroyAllLinks()
	{
		for (auto& [id, link] : _mesh->links) {
			juice_destroy(link.agent);
		}
		_mesh->links.clear();
	}

	void Client::HandleP2PSignal(const Proto::P2PSignal& sig)
	{
		PeerLink* link = FindLink(sig.id);

		switch (sig.kind) {
		case Proto::P2PSignalKind::Description:
			HandleP2PDescription(link, sig);
			break;
		case Proto::P2PSignalKind::Candidate:
			if (link)
				juice_add_remote_candidate(link->agent, sig.payload.c_str());
			break;
		case Proto::P2PSignalKind::GatheringDone:
			if (link)
				juice_set_remote_gathering_done(link->agent);
			break;
		}
	}

	void Client::HandleP2PDescription(PeerLink* link, const Proto::P2PSignal& sig)
	{
		if (!link) {
			// a peer's first message reached out to us; answer with our own
			// agent, remote description first so it takes the controlled role
			link = CreateLink(sig.id);
			if (!link)
				return;
			juice_set_remote_description(link->agent, sig.payload.c_str());
			link->remote_set = true;
			ShareLink(*link, sig.id);
		}
		else if (!link->remote_set) {
			// glare: both ends opened first simultaneously, so both agents are
			// controlling and the ICE role-conflict tie-breaker settles it
			juice_set_remote_description(link->agent, sig.payload.c_str());
			link->remote_set = true;
		}
	}

	void Client::PollPeers()
	{
		std::vector<JuiceEvent>& events = _mesh->scratch;
		{
			std::lock_guard lock(_mesh->mutex);
			events.swap(_mesh->events);
		}

		for (JuiceEvent& ev : events) {
			HandleJuiceEvent(ev);
		}
		events.clear(); // keeps its capacity for the next poll
	}

	void Client::HandleJuiceEvent(JuiceEvent& ev)
	{
		PeerLink* link = FindLink(ev.peer);
		if (!link)
			return; // the link was torn down after this event was queued

		switch (ev.kind) {
		case JuiceEvent::Kind::State:
			HandleLinkState(*link, ev);
			break;
		case JuiceEvent::Kind::Candidate:
			SendServer(Proto::P2PSignal{ ev.peer, Proto::P2PSignalKind::Candidate, std::move(ev.payload) });
			break;
		case JuiceEvent::Kind::GatheringDone:
			SendServer(Proto::P2PSignal{ ev.peer, Proto::P2PSignalKind::GatheringDone, {} });
			break;
		case JuiceEvent::Kind::Recv: {
			auto* bytes = reinterpret_cast<const std::byte*>(ev.payload.data());
			_p2p_inbox.emplace(ev.peer, Proto::P2PMessage{ bytes, bytes + ev.payload.size() });
			break;
		}
		}
	}

	void Client::HandleLinkState(PeerLink& link, const JuiceEvent& ev)
	{
		switch (ev.state) {
		case JUICE_STATE_CONNECTED:
		case JUICE_STATE_COMPLETED:
			link.connected = true;
			FlushLink(link);
			break;
		case JUICE_STATE_DISCONNECTED:
			link.connected = false; // may still recover; sends queue meanwhile
			break;
		case JUICE_STATE_FAILED:
			DestroyLink(ev.peer); // the next SendP2P starts over
			break;
		default:
			break;
		}
	}
}
