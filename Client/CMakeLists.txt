add_executable(
    Client
    ${CMAKE_CURRENT_SOURCE_DIR}/src/weyvelength.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/src/test.cpp)
target_include_directories(Client PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(Client PRIVATE weyvelength_shared)
