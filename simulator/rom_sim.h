#pragma once

#include <verilated.h>

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <random>

class ROMSim {
 public:
  explicit ROMSim(size_t size,
                  CData& valid_o,
                  CData* ready_i,
                  IData* data_o,
                  IData& addr_o);

  void LoadHex(const std::string& filename);

  void Do();

 private:
  CData& valid_o_;
  CData* ready_i_;
  IData* data_o_;
  IData& addr_o_;

  const size_t size_;
  std::vector<IData> mem_;
};
