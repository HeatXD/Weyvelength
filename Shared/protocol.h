#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <variant>
#include <vector>

namespace Weyvelength::Proto {
	struct Heartbeat { uint64_t timestamp; };   // server <-> client heartbeat
	struct AssignClientId { uint32_t id = 0; };  // server -> client: the client's own connection id
	struct AssignRoomId { std::string id; }; // server -> client: the room id the client has joined successfully

	enum class RoomErrorCode : uint8_t {
		AlreadyInRoom,
		NoSuchRoom,
		NotInRoom,
		NotHost, // host-only action attempted by a non-host
		BadRoomData, // key/value over the size limits, or too many keys
		NoSuchMember, // target id is not another member of the room
		RoomClosed, // the room is not joinable right now
		BadPassword, // wrong password on join, or an over-long one on set
		Banned, // the host has barred this client from the room
	};

	struct CreateRoom {}; // client -> server: create a room and join it

	struct JoinRoom { // client -> server: join an existing room by id
		std::string id;
		std::string password; // must match the room's password if one is set
	};

	struct LeaveRoom {}; // client -> server: leave the current room

	struct RoomError { // server -> client: a room request failed
		RoomErrorCode code{};
		std::string context; // extra detail, e.g. the offending room id
	};

	struct RoomChat { // client -> server: broadcast text to the sender's room
		uint32_t from = 0; // server -> client: filled in with the sender's id
		std::string text;
	};

	struct PeerJoined { uint32_t id = 0; }; // server -> client: another client is in the room (live join, or replayed to a joiner per existing member)
	struct PeerLeft { uint32_t id = 0; }; // server -> client: a client left the room; your own id confirms your LeaveRoom
	struct HostChanged { uint32_t id = 0; }; // server -> client: the room's current host

	struct SetRoomData { // client -> server: set one room metadata key; empty value deletes it
		std::string key;
		std::string value;
	};

	struct RoomDataChanged { // server -> client: one room metadata key changed; empty value means deleted
		std::string key;
		std::string value;
	};

	struct SetMemberData { // client -> server: set one key of your own member metadata; empty value deletes it
		std::string key;
		std::string value;
	};

	struct MemberDataChanged { // server -> client: one key of a member's metadata changed; empty value means deleted
		uint32_t id = 0; // whose data
		std::string key;
		std::string value;
	};

	struct KickMember { uint32_t id = 0; }; // client -> server: host-only, remove a member from the room
	struct BanMember { uint32_t id = 0; }; // client -> server: host-only, remove a member and bar them from rejoining
	struct TransferHost { uint32_t id = 0; }; // client -> server: host-only, hand host status to another member
	struct SetRoomJoinable { bool open = true; }; // client -> server: host-only, open or close the room for joining
	struct SetRoomPassword { std::string password; }; // client -> server: host-only; empty clears it

	struct KickedByHost {}; // server -> client: the host removed you from the room
	struct BannedByHost {}; // server -> client: the host removed you and barred you from rejoining

	struct RoomAccessChanged { // server -> client: the room's joinability changed; the password itself never leaves the server
		bool open = true;
		bool passworded = false;
	};

	enum class P2PSignalKind : uint8_t {
		Description,
		Candidate,
		GatheringDone,
	};

	struct P2PSignal { // relayed ICE signaling; id is the target on send, the sender on receive
		uint32_t id = 0;
		P2PSignalKind kind{};
		std::string payload; // sdp text
	};

	struct TurnServer {
		std::string host;
		uint16_t port = 0;
		std::string username;
		std::string password;
	};

	struct IceServers { // server -> client: sent once after connect
		std::string stun_host; // empty = no stun
		uint16_t stun_port = 0;
		std::vector<TurnServer> turn;
	};

	// All traffic on the server connection, both directions. Only append new
	// messages: zpp_bits encodes the variant index, so inserting in the middle
	// breaks peers built against the old order.
	using ServerMessage = std::variant<Heartbeat, AssignClientId, AssignRoomId, CreateRoom, JoinRoom, RoomError, RoomChat,
		LeaveRoom, PeerJoined, PeerLeft, HostChanged, SetRoomData, RoomDataChanged, SetMemberData, MemberDataChanged,
		KickMember, TransferHost, SetRoomJoinable, SetRoomPassword, KickedByHost, RoomAccessChanged, BanMember, BannedByHost,
		P2PSignal, IceServers>;

	// Opaque bytes, one datagram per message; the app defines its own encoding.
	using P2PMessage = std::vector<std::byte>;

	constexpr uint32_t max_p2p_message_size = 1024;
	constexpr uint32_t max_message_size = 1024;

	// Metadata limits, shared by room and member data. One key/value pair per
	// frame, so the pair caps keep every data message under max_message_size.
	constexpr uint32_t max_room_data_key = 128;
	constexpr uint32_t max_room_data_value = 512;
	constexpr uint32_t max_room_data_keys = 64;
	constexpr uint32_t max_member_data_keys = 16;
	constexpr uint32_t max_room_password = 64;
}
