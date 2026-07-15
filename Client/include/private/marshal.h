#pragma once

#include <cstdint>
#include <iterator>
#include <map>
#include <string>

#include "protocol.h"
#include "weyvelength.h"

// Marshalling between the C API's strings/events and the C++ client's types.
// Header-only and internal to the C wrapper; not part of the public C ABI.
namespace Weyvelength::Marshal {
	inline std::string Str(const char* str) // null is an empty string, not a crash
	{
		return str ? std::string(str) : std::string();
	}

	inline const char* Bytes(const std::string* value, uint32_t* len) // null stays null, empty is a valid pointer
	{
		if (!value) {
			if (len)
				*len = 0;
			return nullptr;
		}
		if (len)
			*len = (uint32_t)value->size();
		return value->data();
	}

	inline const char* KeyAt(const std::map<std::string, std::string>& data, uint32_t index, uint32_t* key_len)
	{
		if (index >= data.size()) {
			if (key_len)
				*key_len = 0;
			return nullptr;
		}
		auto it = data.begin();
		std::advance(it, index);
		if (key_len)
			*key_len = (uint32_t)it->first.size();
		return it->first.data();
	}

	// Translate one surfaced server message into the C event; false for the few
	// variants that never reach here, so weyve_next skips to the next.
	inline bool FillEvent(const Proto::ServerMessage& msg, WeyveEvent* out)
	{
		if (auto* beat = std::get_if<Proto::Heartbeat>(&msg)) {
			out->type = WEYVE_EVENT_HEARTBEAT;
			out->data.heartbeat.timestamp = beat->timestamp;
		}
		else if (auto* room = std::get_if<Proto::AssignRoomId>(&msg)) {
			out->type = WEYVE_EVENT_ROOM_ID_ASSIGNED;
			out->data.room_assigned.id = room->id.data();
			out->data.room_assigned.id_len = (uint32_t)room->id.size();
		}
		else if (auto* error = std::get_if<Proto::RoomError>(&msg)) {
			out->type = WEYVE_EVENT_ROOM_ERROR;
			out->data.room_error.code = (WeyveRoomError)error->code;
			out->data.room_error.context = error->context.data();
			out->data.room_error.context_len = (uint32_t)error->context.size();
		}
		else if (auto* chat = std::get_if<Proto::RoomChat>(&msg)) {
			out->type = WEYVE_EVENT_CHAT;
			out->data.chat.from = chat->from;
			out->data.chat.text = chat->text.data();
			out->data.chat.text_len = (uint32_t)chat->text.size();
		}
		else if (auto* joined = std::get_if<Proto::PeerJoined>(&msg)) {
			out->type = WEYVE_EVENT_PEER_JOINED;
			out->data.peer_joined.id = joined->id;
		}
		else if (auto* left = std::get_if<Proto::PeerLeft>(&msg)) {
			out->type = WEYVE_EVENT_PEER_LEFT;
			out->data.peer_left.id = left->id;
		}
		else if (auto* host = std::get_if<Proto::HostChanged>(&msg)) {
			out->type = WEYVE_EVENT_HOST_CHANGED;
			out->data.host_changed.id = host->id;
		}
		else if (auto* data = std::get_if<Proto::RoomDataChanged>(&msg)) {
			out->type = WEYVE_EVENT_ROOM_DATA_CHANGED;
			out->data.room_data.key = data->key.data();
			out->data.room_data.key_len = (uint32_t)data->key.size();
			out->data.room_data.value = data->value.data();
			out->data.room_data.value_len = (uint32_t)data->value.size();
		}
		else if (auto* member = std::get_if<Proto::MemberDataChanged>(&msg)) {
			out->type = WEYVE_EVENT_MEMBER_DATA_CHANGED;
			out->data.member_data.id = member->id;
			out->data.member_data.key = member->key.data();
			out->data.member_data.key_len = (uint32_t)member->key.size();
			out->data.member_data.value = member->value.data();
			out->data.member_data.value_len = (uint32_t)member->value.size();
		}
		else if (std::get_if<Proto::KickedByHost>(&msg)) {
			out->type = WEYVE_EVENT_KICKED;
		}
		else if (std::get_if<Proto::BannedByHost>(&msg)) {
			out->type = WEYVE_EVENT_BANNED;
		}
		else if (auto* access = std::get_if<Proto::RoomAccessChanged>(&msg)) {
			out->type = WEYVE_EVENT_ROOM_ACCESS_CHANGED;
			out->data.room_access.open = access->open;
			out->data.room_access.passworded = access->passworded;
		}
		else {
			return false; // a client->server variant we never receive
		}
		return true;
	}
}
