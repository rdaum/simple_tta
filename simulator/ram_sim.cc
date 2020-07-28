#include "ram_sim.h"

RAMSim::RAMSim(size_t size,
               CData& wstrb_o,
               CData& valid_o,
               CData* ready_i,
               IData* read_data,
               IData& write_data,
               IData& addr_o)
    : size_(size),
      wstrb_o_(wstrb_o),
      valid_o_(valid_o),
      ready_i_(ready_i),
      read_data_(read_data),
      write_data_(write_data),
      addr_o_(addr_o) {
  mem_.resize(size);
}

void RAMSim::Do() {
  if (valid_o_) {
    IData* data = &mem_[addr_o_];
    if (wstrb_o_ != 0) {
      CData* cd = (CData*)data;
      CData* wd = (CData*)&write_data_;
      if (wstrb_o_ & 0x01) {
        cd[0] = wd[0];
      }
      if (wstrb_o_ & 0x02) {
        cd[1] = wd[1];
      }
      if (wstrb_o_ & 0x04) {
        cd[2] = wd[2];
      }
      if (wstrb_o_ & 0x08) {
        cd[3] = wd[3];
      }
    }
    *read_data_ = *data;
  }
  *ready_i_ = valid_o_;
}

void RAMSim::Randomize() {
  for (int i = 0; i < size_; i++) {
    mem_[i] = rand() % 255;
  }
}
