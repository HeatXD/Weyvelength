#pragma once

#include <cstddef>
#include <cstdint>
#include <span>
#include <vector>

#include <thirdparty\zpp_bits\zpp_bits.h>

#include "protocol.h"

// Wire framing shared by both ends of the server connection: a message is
// encoded as [uint32 length][zpp_bits body]. Transport (asio coroutines on the
// server, polled sockets on the client) differs, but this byte layout does not.
namespace Weyvelength::Proto {
	constexpr size_t frame_header_size = sizeof(uint32_t);

	// Encodes a message as a length-prefixed frame ready to write to a socket.
	inline std::vector<std::byte> FrameMessage(const ServerMessage& msg)
	{
		std::vector<std::byte> body;
		zpp::bits::out{ body }(msg).or_throw();

		std::vector<std::byte> frame;
		zpp::bits::out{ frame }((uint32_t)body.size()).or_throw();
		frame.insert(frame.end(), body.begin(), body.end());
		return frame;
	}

	// Reads the body length from a frame header. False if the bytes are
	// unreadable or the length exceeds max_message_size.
	inline bool DecodeFrameLength(std::span<const std::byte> header, uint32_t& len)
	{
		if (failure(zpp::bits::in{ header }(len)))
			return false;
		return len <= max_message_size;
	}
}
