#pragma once

#include <ostream>

class UARTSim {
 public:
  explicit UARTSim(std::ostream& out_stream) : out_(out_stream) {}

  void Push(bool b);

 private:
  enum State { NEED_START, RECV, NEED_STOP };
  State state = NEED_START;
  char x_ = 0;
  unsigned int bit_ = 0;
  std::ostream& out_;
};
