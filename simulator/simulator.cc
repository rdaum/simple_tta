#include <absl/flags/flag.h>
#include <absl/flags/parse.h>
#include <glog/logging.h>
#include <verilated.h>
#include <verilated_fst_c.h>

#include <atomic>
#include <fstream>
#include <iostream>
#include <memory>
#include <regex>
#include <thread>

#include "Vsimtop.h"
#include "clock_gen.h"
#include "ram_sim.h"
#include "uart_sim.h"

ABSL_FLAG(std::string, trace_file, "", "Trace file");

int main(int argc, char** argv) {
  FLAGS_logtostderr = true;
  google::InitGoogleLogging(argv[0]);
  absl::ParseCommandLine(argc, argv);

  Verilated::commandArgs(argc, argv);
  std::unique_ptr<Vsimtop> soc(new Vsimtop);
  ClockGenerator generator(10, 100 /* reset_cycles */, &soc->rst_i,
                           &soc->sysclk_i);

  VerilatedFstC trace;
  if (!absl::GetFlag(FLAGS_trace_file).empty()) {
    Verilated::traceEverOn(true);
    soc->trace(&trace, 99);
    trace.open(absl::GetFlag(FLAGS_trace_file).c_str());
    LOG(INFO) << "Opened trace file: " << absl::GetFlag(FLAGS_trace_file);
  }

  soc->rst_i = 1;

  UARTSim s(std::cout);

  RAMSim sram(1 << 19, soc->sram_wstrb_o, soc->sram_valid_o, &soc->sram_ready_i,
              &soc->sram_data_o, soc->sram_data_i, soc->sram_addr_o);
  while (!Verilated::gotFinish()) {
    generator.Step(&trace);

    soc->eval();

    if (!soc->rst_i & generator.Bus()) {
      sram.Do();
      static int baud_count = 0;
      if (baud_count == 651) {
        s.Push(soc->uart_txd_o);
        baud_count = 0;
      }
      baud_count++;
    }
  }
  exit(EXIT_SUCCESS);
}