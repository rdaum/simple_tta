project(tta-simulator CXX C)
cmake_minimum_required(VERSION 3.28)

set(CMAKE_CXX_STANDARD 17)

find_package(verilator HINTS $ENV{VERILATOR_ROOT} ${VERILATOR_ROOT})


# Produce the stuff that verilator needs by running the synth but only the setup phase.
execute_process(
        COMMAND fusesoc --cores-root=${CMAKE_src_DIR} run --setup --target=synth tta
        WORKING_DIRECTORY ${CMAKE_BINARY_DIR}
)

# Invoke verilator for the simulator
add_library(verilated_sim STATIC)
verilate(verilated_sim
        VERILATOR_ARGS -O3 -Wno-fatal -sv --clk sysclk_i -Wno-TIMESCALEMOD -Wno-WIDTH --trace-fst
        TOP_MODULE simtop
        SOURCES ../simulator/simtop.sv )

add_library(verilated_test STATIC)
verilate(verilated_test
        VERILATOR_ARGS -O3 -Wno-fatal -sv --clk clk_i -Wno-TIMESCALEMOD -Wno-WIDTH --trace-fst
        TOP_MODULE testtop
        SOURCES ../simulator/testtop.sv)
