#pragma once

#include <cstdint>
#include <map>
#include <memory>
#include <queue>
#include <string>
#include <vector>

#include "protocol.h"

namespace Weyvelength {
	struct ClientAsioImpl;

	struct ClientConfig {
		std::string host = "127.0.0.1";
		uint16_t    port = 0;
	};

	struct Client {
		Client();
		~Client();

		bool Connect(ClientConfig& config);

		bool Poll();

		bool Next(Proto::ServerMessage& out);

		bool SendServer(const Proto::ServerMessage& msg);

		bool CreateRoom(); // server replies AssignRoomId or RoomError
		bool JoinRoom(const std::string& id, const std::string& password = {}); // server replies AssignRoomId or RoomError
		bool LeaveRoom(); // server replies PeerLeft carrying our own id, or RoomError
		bool KickMember(uint32_t id); // host-only; the target gets Kicked, the room gets PeerLeft
		bool TransferHost(uint32_t id); // host-only; server replies HostChanged to the room
		bool SetRoomJoinable(bool open); // host-only; server replies RoomAccessChanged to the room
		bool SetRoomPassword(const std::string& password); // host-only; empty clears it
		bool SendChat(const std::string& text); // broadcast to everyone in the current room
		bool SetRoomData(const std::string& key, const std::string& value); // host-only; server replies RoomDataChanged or RoomError
		bool DeleteRoomData(const std::string& key); // host-only; sugar for an empty-value SetRoomData
		bool SetMemberData(const std::string& key, const std::string& value); // our own slots; server replies MemberDataChanged or RoomError
		bool DeleteMemberData(const std::string& key); // sugar for an empty-value SetMemberData

		uint32_t Id() const;  // 0 until the server has assigned one
		const std::string& RoomId() const; // empty until a room has been joined
		uint32_t Host() const; // 0 until a room has been joined
		bool IsHost() const;
		bool RoomJoinable() const; // can others join right now?
		bool RoomPassworded() const; // the flag only; the password itself never reaches clients
		const std::vector<uint32_t>& Members() const; // everyone in the room, ourselves included
		const std::map<std::string, std::string>& RoomData() const;
		const std::string* RoomData(const std::string& key) const; // null if the key is not set
		const std::map<std::string, std::string>* MemberData(uint32_t id) const; // null if the member has set nothing
		const std::string* MemberData(uint32_t id, const std::string& key) const; // null if the key is not set

	private:
		bool PollServer();
		bool DrainServer();
		bool CarveServer();
		bool FlushServer();
		bool DisconnectServer();
		void CacheRoomState(const Proto::ServerMessage& msg);
		void ClearRoomState();

		std::unique_ptr<ClientAsioImpl> _asio;
		std::queue<Proto::ServerMessage> _inbox;
		uint32_t _id = 0;
		std::string _room;
		uint32_t _host = 0;
		bool _room_open = true;
		bool _room_passworded = false;
		std::vector<uint32_t> _members;
		std::map<std::string, std::string> _data;
		std::map<uint32_t, std::map<std::string, std::string>> _member_data;
	};
}
