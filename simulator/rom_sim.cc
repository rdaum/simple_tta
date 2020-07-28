#include "rom_sim.h"

ROMSim::ROMSim(size_t size,
               CData& valid_o,
               CData* ready_i,
               IData* data_o,
               IData& addr_o)
    : size_(size),
      valid_o_(valid_o),
      ready_i_(ready_i),
      data_o_(data_o),
      addr_o_(addr_o) {
  mem_.resize(size);
}

void ROMSim::Do() {
  if (valid_o_) {
    IData* data = &mem_[addr_o_];
    *data_o_ = *data;
  }
  *ready_i_ = valid_o_;
}

void ROMSim::LoadHex(const std::string& filename) {}
