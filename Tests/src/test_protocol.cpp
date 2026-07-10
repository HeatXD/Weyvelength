#define DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN
#include <thirdparty/doctest/doctest.h>

#include <array>
#include <cstddef>
#include <cstdint>
#include <span>
#include <string>
#include <type_traits>
#include <variant>
#include <vector>

#include <thirdparty/zpp_bits/zpp_bits.h>

#include "framing.h"
#include "protocol.h"

using namespace Weyvelength;

// The wire encodes a message as its variant index, so the order below IS the
// protocol. These pins turn the append-only comment in protocol.h into a
// compile error: inserting or reordering an alternative fails right here.
static_assert(std::variant_size_v<Proto::ServerMessage> == 25);
static_assert(std::is_same_v<std::variant_alternative_t<0, Proto::ServerMessage>, Proto::Heartbeat>);
static_assert(std::is_same_v<std::variant_alternative_t<1, Proto::ServerMessage>, Proto::AssignClientId>);
static_assert(std::is_same_v<std::variant_alternative_t<2, Proto::ServerMessage>, Proto::AssignRoomId>);
static_assert(std::is_same_v<std::variant_alternative_t<3, Proto::ServerMessage>, Proto::CreateRoom>);
static_assert(std::is_same_v<std::variant_alternative_t<4, Proto::ServerMessage>, Proto::JoinRoom>);
static_assert(std::is_same_v<std::variant_alternative_t<5, Proto::ServerMessage>, Proto::RoomError>);
static_assert(std::is_same_v<std::variant_alternative_t<6, Proto::ServerMessage>, Proto::RoomChat>);
static_assert(std::is_same_v<std::variant_alternative_t<7, Proto::ServerMessage>, Proto::LeaveRoom>);
static_assert(std::is_same_v<std::variant_alternative_t<8, Proto::ServerMessage>, Proto::PeerJoined>);
static_assert(std::is_same_v<std::variant_alternative_t<9, Proto::ServerMessage>, Proto::PeerLeft>);
static_assert(std::is_same_v<std::variant_alternative_t<10, Proto::ServerMessage>, Proto::HostChanged>);
static_assert(std::is_same_v<std::variant_alternative_t<11, Proto::ServerMessage>, Proto::SetRoomData>);
static_assert(std::is_same_v<std::variant_alternative_t<12, Proto::ServerMessage>, Proto::RoomDataChanged>);
static_assert(std::is_same_v<std::variant_alternative_t<13, Proto::ServerMessage>, Proto::SetMemberData>);
static_assert(std::is_same_v<std::variant_alternative_t<14, Proto::ServerMessage>, Proto::MemberDataChanged>);
static_assert(std::is_same_v<std::variant_alternative_t<15, Proto::ServerMessage>, Proto::KickMember>);
static_assert(std::is_same_v<std::variant_alternative_t<16, Proto::ServerMessage>, Proto::TransferHost>);
static_assert(std::is_same_v<std::variant_alternative_t<17, Proto::ServerMessage>, Proto::SetRoomJoinable>);
static_assert(std::is_same_v<std::variant_alternative_t<18, Proto::ServerMessage>, Proto::SetRoomPassword>);
static_assert(std::is_same_v<std::variant_alternative_t<19, Proto::ServerMessage>, Proto::KickedByHost>);
static_assert(std::is_same_v<std::variant_alternative_t<20, Proto::ServerMessage>, Proto::RoomAccessChanged>);
static_assert(std::is_same_v<std::variant_alternative_t<21, Proto::ServerMessage>, Proto::BanMember>);
static_assert(std::is_same_v<std::variant_alternative_t<22, Proto::ServerMessage>, Proto::BannedByHost>);
static_assert(std::is_same_v<std::variant_alternative_t<23, Proto::ServerMessage>, Proto::P2PSignal>);
static_assert(std::is_same_v<std::variant_alternative_t<24, Proto::ServerMessage>, Proto::IceServers>);

// Same idea for the error enum: the values are wire bytes, append only.
static_assert((uint8_t)Proto::RoomErrorCode::AlreadyInRoom == 0);
static_assert((uint8_t)Proto::RoomErrorCode::NoSuchRoom == 1);
static_assert((uint8_t)Proto::RoomErrorCode::NotInRoom == 2);
static_assert((uint8_t)Proto::RoomErrorCode::NotHost == 3);
static_assert((uint8_t)Proto::RoomErrorCode::BadRoomData == 4);
static_assert((uint8_t)Proto::RoomErrorCode::NoSuchMember == 5);
static_assert((uint8_t)Proto::RoomErrorCode::RoomClosed == 6);
static_assert((uint8_t)Proto::RoomErrorCode::BadPassword == 7);
static_assert((uint8_t)Proto::RoomErrorCode::Banned == 8);

// And the p2p signal kinds.
static_assert((uint8_t)Proto::P2PSignalKind::Description == 0);
static_assert((uint8_t)Proto::P2PSignalKind::Candidate == 1);
static_assert((uint8_t)Proto::P2PSignalKind::GatheringDone == 2);

namespace {
	// Frames a message, then walks the fragment stream and reassembles it the
	// way both peers do, verifying every header along the way.
	Proto::ServerMessage RoundTrip(const Proto::ServerMessage& msg)
	{
		std::vector<std::byte> frames = Proto::FrameMessage(msg);

		std::vector<std::byte> body;
		std::span<const std::byte> remaining{ frames };
		bool more = true;

		while (more) {
			REQUIRE(remaining.size() >= Proto::frame_header_size);

			uint32_t len = 0;
			REQUIRE(Proto::DecodeFrameHeader(remaining.first(Proto::frame_header_size), len, more));
			REQUIRE(remaining.size() >= Proto::frame_header_size + len);
			REQUIRE(Proto::AppendFragment(body, remaining.subspan(Proto::frame_header_size, len)));
			remaining = remaining.subspan(Proto::frame_header_size + len);
		}

		REQUIRE(remaining.empty()); // no bytes past the final fragment

		Proto::ServerMessage out;
		REQUIRE(!failure(zpp::bits::in{ body }(out)));
		REQUIRE(out.index() == msg.index());
		return out;
	}
}

TEST_CASE("heartbeat round trips with its timestamp")
{
	auto out = std::get<Proto::Heartbeat>(RoundTrip(Proto::Heartbeat{ 0x1122334455667788 }));
	CHECK(out.timestamp == 0x1122334455667788);
}

TEST_CASE("identity and room assignment round trip")
{
	CHECK(std::get<Proto::AssignClientId>(RoundTrip(Proto::AssignClientId{ 42 })).id == 42);
	CHECK(std::get<Proto::AssignRoomId>(RoundTrip(Proto::AssignRoomId{ "VY4C3NB9" })).id == "VY4C3NB9");
}

TEST_CASE("room requests round trip")
{
	RoundTrip(Proto::CreateRoom{}); // the index check inside is the whole test
	RoundTrip(Proto::LeaveRoom{});

	auto join = std::get<Proto::JoinRoom>(RoundTrip(Proto::JoinRoom{ "ROOMCODE", "hunter2" }));
	CHECK(join.id == "ROOMCODE");
	CHECK(join.password == "hunter2");

	CHECK(std::get<Proto::JoinRoom>(RoundTrip(Proto::JoinRoom{ "ROOMCODE" })).password.empty());
}

TEST_CASE("room errors round trip with code and context")
{
	auto out = std::get<Proto::RoomError>(RoundTrip(Proto::RoomError{ Proto::RoomErrorCode::NoSuchRoom, "ROOMCODE" }));
	CHECK(out.code == Proto::RoomErrorCode::NoSuchRoom);
	CHECK(out.context == "ROOMCODE");
}

TEST_CASE("chat round trips, spaces and empty text included")
{
	auto out = std::get<Proto::RoomChat>(RoundTrip(Proto::RoomChat{ 7, "two words  and spaces" }));
	CHECK(out.from == 7);
	CHECK(out.text == "two words  and spaces");

	CHECK(std::get<Proto::RoomChat>(RoundTrip(Proto::RoomChat{ 7, "" })).text.empty());
}

TEST_CASE("room events round trip")
{
	CHECK(std::get<Proto::PeerJoined>(RoundTrip(Proto::PeerJoined{ 3 })).id == 3);
	CHECK(std::get<Proto::PeerLeft>(RoundTrip(Proto::PeerLeft{ 3 })).id == 3);
	CHECK(std::get<Proto::HostChanged>(RoundTrip(Proto::HostChanged{ 3 })).id == 3);
}

TEST_CASE("host actions round trip")
{
	CHECK(std::get<Proto::KickMember>(RoundTrip(Proto::KickMember{ 3 })).id == 3);
	CHECK(std::get<Proto::BanMember>(RoundTrip(Proto::BanMember{ 3 })).id == 3);
	CHECK(std::get<Proto::TransferHost>(RoundTrip(Proto::TransferHost{ 3 })).id == 3);
	CHECK(std::get<Proto::SetRoomJoinable>(RoundTrip(Proto::SetRoomJoinable{ false })).open == false);

	CHECK(std::get<Proto::SetRoomPassword>(RoundTrip(Proto::SetRoomPassword{ "hunter2" })).password == "hunter2");
	CHECK(std::get<Proto::SetRoomPassword>(RoundTrip(Proto::SetRoomPassword{ "" })).password.empty()); // empty = clear
}

TEST_CASE("room access events round trip")
{
	RoundTrip(Proto::KickedByHost{}); // the index check inside is the whole test
	RoundTrip(Proto::BannedByHost{});

	auto access = std::get<Proto::RoomAccessChanged>(RoundTrip(Proto::RoomAccessChanged{ false, true }));
	CHECK(access.open == false);
	CHECK(access.passworded == true);
}

TEST_CASE("p2p signaling round trips")
{
	auto sig = std::get<Proto::P2PSignal>(RoundTrip(Proto::P2PSignal{ 3, Proto::P2PSignalKind::Candidate, "a=candidate:1 1 UDP 2122317823 192.168.0.10 50000 typ host" }));
	CHECK(sig.id == 3);
	CHECK(sig.kind == Proto::P2PSignalKind::Candidate);
	CHECK(sig.payload.starts_with("a=candidate"));

	auto ice = std::get<Proto::IceServers>(RoundTrip(Proto::IceServers{ "stun.example.net", 19302, { { "turn.example.net", 3478, "user", "pass" } } }));
	CHECK(ice.stun_host == "stun.example.net");
	CHECK(ice.stun_port == 19302);
	REQUIRE(ice.turn.size() == 1);
	CHECK(ice.turn[0].host == "turn.example.net");
	CHECK(ice.turn[0].port == 3478);
	CHECK(ice.turn[0].username == "user");
	CHECK(ice.turn[0].password == "pass");
}

TEST_CASE("data messages round trip, empty value (= delete) included")
{
	auto set = std::get<Proto::SetRoomData>(RoundTrip(Proto::SetRoomData{ "stage", "training" }));
	CHECK(set.key == "stage");
	CHECK(set.value == "training");

	auto del = std::get<Proto::RoomDataChanged>(RoundTrip(Proto::RoomDataChanged{ "stage", "" }));
	CHECK(del.key == "stage");
	CHECK(del.value.empty());

	auto mine = std::get<Proto::SetMemberData>(RoundTrip(Proto::SetMemberData{ "char", "ryu" }));
	CHECK(mine.key == "char");
	CHECK(mine.value == "ryu");

	auto member = std::get<Proto::MemberDataChanged>(RoundTrip(Proto::MemberDataChanged{ 5, "char", "akuma" }));
	CHECK(member.id == 5);
	CHECK(member.key == "char");
	CHECK(member.value == "akuma");
}

TEST_CASE("the largest allowed data pair fits under the frame cap")
{
	std::string key(Proto::max_room_data_key, 'k');
	std::string value(Proto::max_room_data_value, 'v');

	// MemberDataChanged is the biggest data message on the wire: it carries
	// the owner id on top of the maxed key/value pair.
	std::vector<std::byte> frame = Proto::FrameMessage(Proto::MemberDataChanged{ 0xFFFFFFFF, key, value });
	CHECK(frame.size() - Proto::frame_header_size <= Proto::max_message_size);

	frame = Proto::FrameMessage(Proto::RoomDataChanged{ key, value });
	CHECK(frame.size() - Proto::frame_header_size <= Proto::max_message_size);

	RoundTrip(Proto::MemberDataChanged{ 0xFFFFFFFF, key, value });
}

TEST_CASE("oversized messages fragment and reassemble")
{
	std::string sdp(3 * Proto::max_message_size, 's'); // forces a multi-fragment stream
	auto out = std::get<Proto::P2PSignal>(RoundTrip(Proto::P2PSignal{ 7, Proto::P2PSignalKind::Description, sdp }));
	CHECK(out.payload == sdp); // RoundTrip walked headers and reassembled along the way
}

TEST_CASE("single-fragment frames keep the plain length header")
{
	std::vector<std::byte> frame = Proto::FrameMessage(Proto::Heartbeat{ 1 });

	uint32_t len = 0;
	bool more = true;
	REQUIRE(Proto::DecodeFrameHeader(std::span<const std::byte>{ frame }.first(Proto::frame_header_size), len, more));
	CHECK(!more); // top bit clear: byte-identical to the pre-fragmentation framing
	CHECK(len == frame.size() - Proto::frame_header_size);
}

TEST_CASE("frame headers over the cap are rejected, reassembly is capped")
{
	std::vector<std::byte> header;
	zpp::bits::out{ header }(uint32_t{ Proto::max_message_size + 1 }).or_throw();

	uint32_t len = 0;
	bool more = false;
	CHECK(!Proto::DecodeFrameHeader(header, len, more));

	header.clear();
	zpp::bits::out{ header }(Proto::max_message_size | Proto::frame_more_flag).or_throw();
	CHECK(Proto::DecodeFrameHeader(header, len, more));
	CHECK(more);
	CHECK(len == Proto::max_message_size);

	std::vector<std::byte> body(Proto::max_reassembled_size - 10);
	std::array<std::byte, 20> extra{};
	CHECK(!Proto::AppendFragment(body, extra)); // one byte over the reassembly cap fails
}
