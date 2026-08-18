# Weyvelength
### C/C++ Rooms and Peer To Peer Networking SDK

Weyvelength gets players into a room together and then out of your way.

You run one small relay server. Players connect, create or join a room by code, and the server keeps everyone's view of that room in sync: who is in it, who hosts it, whatever metadata you hang off it. It also relays the ICE signaling peers need to find each other, and [libjuice](https://github.com/paullouisageneau/libjuice) opens a direct UDP path between them, punching through NATs along the way.

You drive it through a plain C API. A room holds any number of players, and you address each by peer id. The first message you send someone opens a direct link to them, and nothing is negotiated for peers you never talk to. What you put in those bytes is up to you.

## Project Goals
### Done
- Rooms
	- Create or join by code, optionally password protected
	- Open or close a room to new joiners
	- Host status, handed on automatically when the host leaves, or transferred on purpose
	- Kick and ban
	- Room metadata and per member metadata, mirrored to everyone in the room
	- Room chat
- Room browser
	- Opt in listing, off by default so a room code stays the secret it is
	- Listing slots, the part of a room non members can read
	- Filtered queries over those slots
- Client identity
	- A display name, fixed while in a room, carried alongside the id everywhere it appears
- Peer to peer connections
	- Direct links to any room member, opened by the first message you send them
	- ICE negotiation relayed through the server, no signaling service of your own
	- Configurable STUN/TURN servers, handed to clients by the server
	- Credentials that rotate while the server keeps running
- Poll based event system, no callbacks across the C boundary
- Automated builds for Windows, Linux and macOS

### Maybe Later
- Reliability/ordering options on top of the raw UDP path
- Reconnecting to a room after a dropped connection

## Getting Started
### Docs
- The API lives in a single header: [`Client/include/weyvelength.h`](Client/include/weyvelength.h)
- Running the server: [`docs/server.md`](docs/server.md)
- STUN/TURN with coturn, Violet or Cloudflare: [`docs/ice_servers.md`](docs/ice_servers.md)
- Look at [`ClientExample`](ClientExample) to see how it all fits together, it exercises every function and event in the API

The shape of it:

```c
WeyveClient *client = weyve_client_create();
weyve_connect(client, "127.0.0.1", 5555);

weyve_set_name(client, "Ada"); // before joining a room
weyve_create_room(client);     // or weyve_join_room(client, "TKBPECC3", NULL)

while (running) {
    if (!weyve_poll(client))
        break; // the connection is gone

    WeyveEvent event;
    while (weyve_next(client, &event)) {
        // handle event.type
    }

    uint32_t from, len;
    const uint8_t *data;
    while ((data = weyve_next_p2p(client, &from, &len)) != NULL) {
        // a datagram straight from a peer
    }
}

weyve_client_destroy(client);
```

`weyve_poll` is instant and never blocks, so it drops straight into a frame loop. Event payload pointers stay valid until the next `weyve_poll` or `weyve_next`, so read them before polling again. All calls for a client must happen on the thread that drives `weyve_poll`.

## Building Weyvelength
### Prerequisites
To build Weyvelength, make sure you have the following installed:

1. **CMake** (version 3.15 or higher)
2. **C++ Compiler** with C++20 support:
   - **GCC** or **Clang** (Linux/macOS)
   - **MSVC** (Visual Studio) for Windows

libjuice and asio come along as submodules and are built and linked statically, so there is no package manager to set up.

### Step-by-Step Instructions

#### 1. Clone the Repository
```sh
git clone --recurse-submodules https://github.com/HeatXD/Weyvelength.git
cd Weyvelength
```

If you already cloned without `--recurse-submodules`, run `git submodule update --init --recursive` instead.

#### 2. Configure Build Options
- `WEYVELENGTH_BUILD_SHARED`: also build the client SDK as a shared library. Defaults to `ON`.
- `WEYVELENGTH_BUILD_TESTS`: build the test target. Defaults to `ON`.

Use `cmake` with `-D` flags:

```sh
cmake -S . -B build -DWEYVELENGTH_BUILD_TESTS=OFF
```

#### 3. Build the Project
```sh
cmake --build build
```

On Windows you can also just open `Weyvelength.sln` in Visual Studio, which builds the library, the server and the examples without CMake.

### Build Output
- **Client SDK**: `weyvelength_client` static, and `weyvelength` shared when `WEYVELENGTH_BUILD_SHARED` is on
- **Server**: `Server`, a standalone executable, see [`docs/server.md`](docs/server.md)
- **Tests**: `Tests`, a doctest binary covering the wire protocol and the C ABI marshalling

Prebuilt server and client SDK artifacts for Windows, Linux and macOS are attached to each release.

---

## License
Weyvelength is licensed under the BSD-2-Clause license
[Read about it here](https://opensource.org/license/bsd-2-clause).
