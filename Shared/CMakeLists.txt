# Shared protocol/framing headers plus the vendored asio and zpp_bits. Asio's
# root goes on the path too: its internal "asio/..." includes resolve through
# the include stack on msvc but need the explicit directory everywhere else.
add_library(weyvelength_shared INTERFACE)
target_include_directories(
    weyvelength_shared
    INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}
    INTERFACE ${CMAKE_CURRENT_SOURCE_DIR}/thirdparty/asio)
target_link_libraries(weyvelength_shared INTERFACE Threads::Threads)

if(WIN32)
    target_link_libraries(weyvelength_shared INTERFACE ws2_32 mswsock) # asio; msvc auto-links these, mingw does not
endif()
