set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE) # juice-static only; we don't need the juice DLL target
set(NO_TESTS ON CACHE BOOL "" FORCE)
set(NO_SERVER ON CACHE BOOL "" FORCE) # ICE-only; the STUN/TURN server bundled in libjuice is unused here
add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/thirdparty/libjuice)
set_target_properties(juice PROPERTIES EXCLUDE_FROM_ALL ON) # only juice-static is used here
set_property(TARGET juice-static PROPERTY POSITION_INDEPENDENT_CODE ON) # linked into the shared SDK below

set(WEYVELENGTH_CLIENT_SOURCES
    ${CMAKE_CURRENT_SOURCE_DIR}/src/client.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/src/p2p_mesh.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/src/weyvelength.cpp)

# The client SDK: everything behind weyvelength.h. A static lib so other
# projects (e.g. game clients) can link it without vendoring the sources.
add_library(weyvelength_client STATIC ${WEYVELENGTH_CLIENT_SOURCES})
target_include_directories(
    weyvelength_client
    PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include
    PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include/private)
target_link_libraries(weyvelength_client PRIVATE weyvelength_shared juice-static)

# Same SDK as a shared library, for FFI consumers that can't link C++ objects.
# juice is still linked in statically, so the result has no extra runtime deps.
option(WEYVELENGTH_BUILD_SHARED "Build the client SDK as a shared library too" ON)
if(WEYVELENGTH_BUILD_SHARED)
    add_library(weyvelength_client_shared SHARED ${WEYVELENGTH_CLIENT_SOURCES})
    set_target_properties(
        weyvelength_client_shared
        PROPERTIES OUTPUT_NAME weyvelength
                   WINDOWS_EXPORT_ALL_SYMBOLS OFF) # WEYVE_API marks the C ABI; nothing else is public
    target_include_directories(
        weyvelength_client_shared
        PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include
        PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include/private)
    target_compile_definitions(
        weyvelength_client_shared
        PUBLIC WEYVE_DLL
        PRIVATE WEYVE_DLL_EXPORT)
    target_link_libraries(weyvelength_client_shared PRIVATE weyvelength_shared juice-static)
endif()
