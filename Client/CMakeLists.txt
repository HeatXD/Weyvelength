set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE) # juice-static only; we don't need the juice DLL target
set(NO_TESTS ON CACHE BOOL "" FORCE)
set(NO_SERVER ON CACHE BOOL "" FORCE) # ICE-only; the STUN/TURN server bundled in libjuice is unused here
add_subdirectory(${CMAKE_CURRENT_SOURCE_DIR}/thirdparty/libjuice)
set_target_properties(juice PROPERTIES EXCLUDE_FROM_ALL ON) # only juice-static is used here

# The client SDK: everything behind weyvelength.h. A static lib so other
# projects (e.g. game clients) can link it without vendoring the sources.
add_library(
    weyvelength_client STATIC
    ${CMAKE_CURRENT_SOURCE_DIR}/src/client.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/src/p2p_mesh.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/src/weyvelength.cpp)
target_include_directories(
    weyvelength_client
    PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include
    PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include/private)
target_link_libraries(weyvelength_client PRIVATE weyvelength_shared juice-static)
