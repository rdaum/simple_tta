#pragma once

#include <verilated.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <random>

class RAMSim {
 public:
  explicit RAMSim(size_t size,
                  CData& wstrb_o,
                  CData& valid_o,
                  CData* ready_i,
                  IData* read_data,
                  IData& write_data,
                  IData& addr_o);

  // Fill memory with garbage to simulate what real memory often looks like.
  void Randomize();

  void Do();

  std::vector<IData>& mem() { return mem_; }

 private:
  CData &wstrb_o_, &valid_o_;
  CData* ready_i_;
  IData* read_data_;
  IData& write_data_;
  IData& addr_o_;

  const size_t size_;
  std::vector<IData> mem_;
};
