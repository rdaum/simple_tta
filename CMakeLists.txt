# Enable Hunter package management.
include("cmake/HunterGate.cmake")
HunterGate(
        URL "https://github.com/cpp-pm/hunter/archive/v0.23.250.tar.gz"
        SHA1 "0e6ce3072a280110f33162e0265fb3796652673f"
)


cmake_minimum_required(VERSION 3.16)
project(tta)

add_subdirectory(rtl)
add_subdirectory(simulator)
