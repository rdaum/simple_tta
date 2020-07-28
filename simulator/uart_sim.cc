#include "uart_sim.h"

void UARTSim::Push(bool b) {
  switch (state) {
    case NEED_START:
      if (!b)
        state = RECV;
      break;
    case RECV: {
      x_ |= ((int)b << bit_);
      bit_++;
      if (bit_ == 8) {
        state = NEED_STOP;
      }
    } break;
    case NEED_STOP:
      if (b) {
        state = NEED_START;
        out_ << x_ << std::flush;
        bit_ = 0;
        x_ = 0;
        break;
      }
  }
}
