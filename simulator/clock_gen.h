#pragma once

#include <verilated.h>

class VerilatedVcdC;

class ClockGenerator {
 public:
  ClockGenerator(const int divisor,
                 const int reset_cycles,
                 CData* reset,
                 CData* clk_bus)
      : divisor_(divisor),
        reset_steps_(reset_cycles * divisor),
        reset_(reset),
        clk_bus_(clk_bus) {}

  ClockGenerator(ClockGenerator&) = delete;

  void Step(VerilatedFstC* trace = nullptr);

  bool Bus() const { return posedge_bus_; }
  const int step() const { return step_; }
  const int cycles() const { return cycle_; }

 private:
  const int divisor_;
  const int reset_steps_;

  CData* reset_;
  CData* clk_bus_;

  bool posedge_bus_ = false;
  int step_ = 0;
  int cycle_ = 0;
};
