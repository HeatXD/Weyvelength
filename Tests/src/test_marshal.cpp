#include <thirdparty/doctest/doctest.h>

#include <map>
#include <string>
#include <variant>

#include "marshal.h"

using namespace Weyvelength;

// A new alternative that reaches the client needs a case in FillEvent, or it
// falls through to false; these pins flag a variant or error-enum change.
static_assert(std::variant_size_v<Proto::ServerMessage> == 25);
static_assert((int)WEYVE_ROOM_ERROR_ALREADY_IN_ROOM == (int)Proto::RoomErrorCode::AlreadyInRoom);
static_assert((int)WEYVE_ROOM_ERROR_BANNED == (int)Proto::RoomErrorCode::Banned);

namespace {
	bool Surfaced(const Proto::ServerMessage& msg)
	{
		WeyveEvent event{};
		return Marshal::FillEvent(msg, &event);
	}
}

TEST_CASE("heartbeat maps to a pong event")
{
	Proto::ServerMessage msg = Proto::Heartbeat{ 0x1122334455667788 };
	WeyveEvent e{};
	REQUIRE(Marshal::FillEvent(msg, &e));
	CHECK(e.type == WEYVE_EVENT_HEARTBEAT);
	CHECK(e.data.heartbeat.timestamp == 0x1122334455667788);
}

TEST_CASE("assigned room id is surfaced as a borrowed byte range")
{
	Proto::ServerMessage msg = Proto::AssignRoomId{ "VY4C3NB9" };
	WeyveEvent e{};
	REQUIRE(Marshal::FillEvent(msg, &e));
	CHECK(e.type == WEYVE_EVENT_ROOM_ID_ASSIGNED);
	CHECK(std::string(e.data.room_assigned.id, e.data.room_assigned.id_len) == "VY4C3NB9");
	CHECK(e.data.room_assigned.id == std::get<Proto::AssignRoomId>(msg).id.data());
}

TEST_CASE("room error carries the mapped code and context")
{
	Proto::ServerMessage msg = Proto::RoomError{ Proto::RoomErrorCode::NoSuchRoom, "ROOMCODE" };
	WeyveEvent e{};
	REQUIRE(Marshal::FillEvent(msg, &e));
	CHECK(e.type == WEYVE_EVENT_ROOM_ERROR);
	CHECK(e.data.room_error.code == WEYVE_ROOM_ERROR_NO_SUCH_ROOM);
	CHECK(std::string(e.data.room_error.context, e.data.room_error.context_len) == "ROOMCODE");
}

TEST_CASE("chat carries the sender and text, spaces included")
{
	Proto::ServerMessage msg = Proto::RoomChat{ 7, "two words  and spaces" };
	WeyveEvent e{};
	REQUIRE(Marshal::FillEvent(msg, &e));
	CHECK(e.type == WEYVE_EVENT_CHAT);
	CHECK(e.data.chat.from == 7);
	CHECK(std::string(e.data.chat.text, e.data.chat.text_len) == "two words  and spaces");
}

TEST_CASE("peer membership events carry the client id")
{
	Proto::ServerMessage joined = Proto::PeerJoined{ 3 };
	Proto::ServerMessage left = Proto::PeerLeft{ 3 };
	Proto::ServerMessage host = Proto::HostChanged{ 3 };
	WeyveEvent e{};

	REQUIRE(Marshal::FillEvent(joined, &e));
	CHECK(e.type == WEYVE_EVENT_PEER_JOINED);
	CHECK(e.data.peer_joined.id == 3);

	REQUIRE(Marshal::FillEvent(left, &e));
	CHECK(e.type == WEYVE_EVENT_PEER_LEFT);
	CHECK(e.data.peer_left.id == 3);

	REQUIRE(Marshal::FillEvent(host, &e));
	CHECK(e.type == WEYVE_EVENT_HOST_CHANGED);
	CHECK(e.data.host_changed.id == 3);
}

TEST_CASE("room data change carries key/value, empty value means delete")
{
	Proto::ServerMessage set = Proto::RoomDataChanged{ "stage", "training" };
	WeyveEvent e{};
	REQUIRE(Marshal::FillEvent(set, &e));
	CHECK(e.type == WEYVE_EVENT_ROOM_DATA_CHANGED);
	CHECK(std::string(e.data.room_data.key, e.data.room_data.key_len) == "stage");
	CHECK(std::string(e.data.room_data.value, e.data.room_data.value_len) == "training");

	Proto::ServerMessage del = Proto::RoomDataChanged{ "stage", "" };
	REQUIRE(Marshal::FillEvent(del, &e));
	CHECK(e.data.room_data.value_len == 0);
}

TEST_CASE("member data change carries the owner id, key and value")
{
	Proto::ServerMessage msg = Proto::MemberDataChanged{ 5, "char", "akuma" };
	WeyveEvent e{};
	REQUIRE(Marshal::FillEvent(msg, &e));
	CHECK(e.type == WEYVE_EVENT_MEMBER_DATA_CHANGED);
	CHECK(e.data.member_data.id == 5);
	CHECK(std::string(e.data.member_data.key, e.data.member_data.key_len) == "char");
	CHECK(std::string(e.data.member_data.value, e.data.member_data.value_len) == "akuma");
}

TEST_CASE("kick, ban and access notices map to their data-less events")
{
	Proto::ServerMessage kicked = Proto::KickedByHost{};
	Proto::ServerMessage banned = Proto::BannedByHost{};
	Proto::ServerMessage access = Proto::RoomAccessChanged{ false, true };
	WeyveEvent e{};

	REQUIRE(Marshal::FillEvent(kicked, &e));
	CHECK(e.type == WEYVE_EVENT_KICKED);

	REQUIRE(Marshal::FillEvent(banned, &e));
	CHECK(e.type == WEYVE_EVENT_BANNED);

	REQUIRE(Marshal::FillEvent(access, &e));
	CHECK(e.type == WEYVE_EVENT_ROOM_ACCESS_CHANGED);
	CHECK(e.data.room_access.open == false);
	CHECK(e.data.room_access.passworded == true);
}

TEST_CASE("client->server and transport variants are not surfaced")
{
	CHECK(!Surfaced(Proto::AssignClientId{ 5 }));
	CHECK(!Surfaced(Proto::CreateRoom{}));
	CHECK(!Surfaced(Proto::JoinRoom{ "ROOMCODE" }));
	CHECK(!Surfaced(Proto::SetRoomData{ "k", "v" }));
	CHECK(!Surfaced(Proto::P2PSignal{ 1, Proto::P2PSignalKind::Candidate, "x" }));
	CHECK(!Surfaced(Proto::IceServers{ "stun", 3478 }));
}

TEST_CASE("Str turns a C string into std::string, null into empty")
{
	CHECK(Marshal::Str(nullptr).empty());
	CHECK(Marshal::Str("") == "");
	CHECK(Marshal::Str("abc") == "abc");
}

TEST_CASE("Bytes borrows the value, null stays null, empty is a valid pointer")
{
	uint32_t len = 123;
	CHECK(Marshal::Bytes(nullptr, &len) == nullptr);
	CHECK(len == 0);

	std::string value = "payload";
	CHECK(Marshal::Bytes(&value, &len) == value.data());
	CHECK(len == value.size());

	std::string empty;
	CHECK(Marshal::Bytes(&empty, &len) != nullptr);
	CHECK(len == 0);
}

TEST_CASE("KeyAt walks the keys in order, past the end is null")
{
	std::map<std::string, std::string> data{ { "alpha", "1" }, { "beta", "2" } };
	uint32_t len = 0;

	const char* first = Marshal::KeyAt(data, 0, &len);
	CHECK(std::string(first, len) == "alpha");

	const char* second = Marshal::KeyAt(data, 1, &len);
	CHECK(std::string(second, len) == "beta");

	CHECK(Marshal::KeyAt(data, 2, &len) == nullptr);
	CHECK(len == 0);
}
