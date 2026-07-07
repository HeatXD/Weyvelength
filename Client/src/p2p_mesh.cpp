#include "weyvelength.h"
#include "p2p_mesh.h"

#include <algorithm>
#include <cstring>

// The p2p half of the client: a lazily built mesh of libjuice links,
// signaled through the server as P2PSignal frames.
namespace Weyvelength {

	constexpr uint32_t max_connection_attempts = 3; // ICE tries per peer before we give up

	// Juice callbacks run on juice's threads; they only queue, Poll does the rest.
	static void PushJuiceEvent(juice_agent_t* agent, void* user_ptr, JuiceEvent ev)
	{
		auto* ctx = static_cast<JuiceCallbackContext*>(user_ptr);
		ev.agent = agent;
		ev.peer = ctx->peer;

		std::lock_guard lock(ctx->mesh->mutex);
		ctx->mesh->events.push_back(std::move(ev));
	}

	static void OnJuiceState(juice_agent_t* agent, juice_state_t state, void* user_ptr)
	{
		PushJuiceEvent(agent, user_ptr, { .kind = JuiceEvent::Kind::State, .state = state });
	}

	static void OnJuiceCandidate(juice_agent_t* agent, const char* sdp, void* user_ptr)
	{
		auto* bytes = (const std::byte*)sdp;
		PushJuiceEvent(agent, user_ptr, { .kind = JuiceEvent::Kind::Candidate, .payload = { bytes, bytes + std::strlen(sdp) } });
	}

	static void OnJuiceGatheringDone(juice_agent_t* agent, void* user_ptr)
	{
		PushJuiceEvent(agent, user_ptr, { .kind = JuiceEvent::Kind::GatheringDone });
	}

	static void OnJuiceRecv(juice_agent_t* agent, const char* data, size_t size, void* user_ptr)
	{
		auto* bytes = (const std::byte*)data;
		PushJuiceEvent(agent, user_ptr, { .kind = JuiceEvent::Kind::Recv, .payload = { bytes, bytes + size } });
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

		return juice_send(link->agent, (const char*)msg.data(), msg.size()) == JUICE_ERR_SUCCESS;
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
		if (_mesh->attempts[id] >= max_connection_attempts)
			return nullptr; // gave up on this peer until it reconnects or the room resets

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

		_mesh->attempts[id]++; // count only agents that actually started an ICE round
		return &_mesh->links.emplace(id, std::move(link)).first->second;
	}

	// Sends the local description; gathering then trickles the candidates.
	bool Client::ShareLink(PeerLink& link, uint32_t id)
	{
		char sdp[JUICE_MAX_SDP_STRING_LEN];
		if (juice_get_local_description(link.agent, sdp, sizeof(sdp)) != JUICE_ERR_SUCCESS)
			return false;

		SendServer(Proto::P2PSignal{ id, Proto::P2PSignalKind::Description, sdp });
		return juice_gather_candidates(link.agent) == JUICE_ERR_SUCCESS;
	}

	void Client::FlushLink(PeerLink& link)
	{
		for (const std::vector<std::byte>& data : link.outbox) {
			juice_send(link.agent, (const char*)data.data(), data.size());
		}
		link.outbox.clear();
	}

	void Client::DestroyLink(uint32_t id)
	{
		auto it = _mesh->links.find(id);
		if (it == _mesh->links.end())
			return;

		juice_destroy(it->second.agent); // joins the callbacks, so the ctx is safe to free
		_mesh->links.erase(it);
	}

	void Client::DestroyAllLinks()
	{
		for (auto& [id, link] : _mesh->links) {
			juice_destroy(link.agent);
		}
		_mesh->links.clear();
		_mesh->attempts.clear();
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
		// a second description on a link that already spent its one
		// juice_set_remote_description is the peer redialing with a fresh agent
		// (theirs died, ours survived); ours is pinned to the old credentials,
		// so tear it down and answer the new one
		if (link && link->remote_set) {
			DestroyLink(sig.id);
			link = nullptr;
		}

		bool answering = !link;
		if (answering) {
			_mesh->attempts.erase(sig.id); // an inbound dial proves the peer is alive; answering is always allowed
			link = CreateLink(sig.id); // a peer reached out with no link of ours yet
			if (!link)
				return;
		}

		if (!link->remote_set) { // glare aside, apply the remote description once
			juice_set_remote_description(link->agent, sig.payload.c_str());
			link->remote_set = true;
		}

		if (answering)
			ShareLink(*link, sig.id); // remote set first = we take the controlled role
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
		if (!link || link->agent != ev.agent)
			return; // the link was torn down (or rebuilt) after this event was queued

		switch (ev.kind) {
		case JuiceEvent::Kind::State:
			HandleLinkState(*link, ev);
			break;
		case JuiceEvent::Kind::Candidate:
			SendServer(Proto::P2PSignal{ ev.peer, Proto::P2PSignalKind::Candidate, std::string{ (const char*)ev.payload.data(), ev.payload.size() } });
			break;
		case JuiceEvent::Kind::GatheringDone:
			SendServer(Proto::P2PSignal{ ev.peer, Proto::P2PSignalKind::GatheringDone, {} });
			break;
		case JuiceEvent::Kind::Recv:
			_p2p_inbox.emplace(ev.peer, std::move(ev.payload));
			break;
		}
	}

	void Client::HandleLinkState(PeerLink& link, const JuiceEvent& ev)
	{
		switch (ev.state) {
		case JUICE_STATE_CONNECTED:
		case JUICE_STATE_COMPLETED:
			link.connected = true;
			_mesh->attempts.erase(ev.peer); // success clears the budget; a later drop retries fresh
			FlushLink(link);
			break;
		case JUICE_STATE_DISCONNECTED:
			link.connected = false; // may still recover; sends queue meanwhile
			break;
		case JUICE_STATE_FAILED:
			DestroyLink(ev.peer); // the next SendP2P retries, up to max_connection_attempts
			break;
		default:
			break;
		}
	}
}
