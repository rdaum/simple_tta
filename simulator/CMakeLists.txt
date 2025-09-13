project(tta_simulator CXX C)
cmake_minimum_required(VERSION 3.28)

set(CMAKE_CXX_STANDARD 17)

hunter_add_package(glog)
find_package(glog CONFIG REQUIRED)

# Process Abseil's CMake build system
add_subdirectory(abseil-cpp)



set(RTL_DIR ${CMAKE_SOURCE_DIR}/rtl)

add_library(tta_sim_support assembler.cc assembler.h uart_sim.h uart_sim.cc clock_gen.cc clock_gen.h ram_sim.h ram_sim.cc rom_sim.h rom_sim.cc)
target_include_directories(tta_sim_support PUBLIC
        ${VERILATOR_OUTPUT_DIR}
        ${GLOG_ROOT}/include
        /usr/share/verilator/include/
        /usr/share/verilator/include/vltstd)
target_link_libraries(tta_sim_support)
add_compile_definitions(VL_THREADED)
add_executable(tta_sim
        simulator.cc)
target_link_libraries(tta_sim tta_sim_support verilated_sim)
target_include_directories(tta_sim PUBLIC
        ${VERILATOR_OUTPUT_DIR}
        ${GLOG_ROOT}/include
        /usr/share/verilator/include/
        /usr/share/verilator/include/vltstd)
target_link_libraries(tta_sim
        glog::glog
        absl::flags
        absl::flags_parse
        )

hunter_add_package(GTest)
find_package(GTest CONFIG REQUIRED)
add_executable(tta_test tta_test.cc)
add_dependencies(tta_test verilated_sim)
target_include_directories(tta_test PUBLIC
        ${VERILATOR_OUTPUT_DIR}
        ${GLOG_ROOT}/include
        /usr/share/verilator/include/
        /usr/share/verilator/include/vltstd
        )
target_link_libraries(tta_test
        PUBLIC
        tta_sim_support
        verilated_test
        GTest::gtest_main GTest::gmock_main
        glog::glog
        absl::flags
        absl::flags_parse
        )

