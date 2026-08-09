cmake_minimum_required(VERSION 3.15...4.0 FATAL_ERROR)
project(Weyvelength VERSION 0.1 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF) # strict c++20 everywhere, no gnu++20 on gcc/clang

find_package(Threads REQUIRED)

add_subdirectory(Shared)
add_subdirectory(Client)
add_subdirectory(Server)
