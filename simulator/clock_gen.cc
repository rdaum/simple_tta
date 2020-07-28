#include "clock_gen.h"

#include <glog/logging.h>
#include <verilated_fst_c.h>

void ClockGenerator::Step(VerilatedFstC* trace) {
  // Run for some clock cycles in reset before booting...
  if (step_ > reset_steps_ && *reset_) {
    LOG(INFO) << "Releasing reset";
    *reset_ = 0;
  }

  posedge_bus_ = false;

  if (step_ % divisor_ == 0) {
    if (!*clk_bus_) {
      posedge_bus_ = true;
      cycle_++;
    }
    *clk_bus_ = !*clk_bus_;
  }

  if (trace) {
    trace->dump(step_);
    trace->flush();
  }
  step_++;
}
