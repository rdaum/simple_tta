#pragma once

#include <glog/logging.h>

#include <cstdint>
#include <optional>
#include <vector>

enum class ALUOp {
  ALU_NOP = 0x000,
  ALU_ADD = 0x001,
  ALU_SUB = 0x002,
  ALU_MUL = 0x003,
  ALU_DIV = 0x004,
  ALU_MOD = 0x005,
  ALU_EQL = 0x006,
  ALU_SL = 0x007,
  ALU_SR = 0x008,
  ALU_SRA = 0x009,
  ALU_NOT = 0x00a,
  ALU_AND = 0x00b,
  ALU_OR = 0x00c,
  ALU_XOR = 0x00d,
  ALU_GT = 0x00e,
  ALU_LT = 0x00f
};

enum class Unit {
  UNIT_NONE = 0,
  UNIT_STACK_PUSH_POP = 1,
  UNIT_STACK_INDEX = 2,
  UNIT_REGISTER = 3,
  UNIT_ALU_LEFT = 4,
  UNIT_ALU_RIGHT = 5,
  UNIT_ALU_OPERATOR = 6,
  UNIT_ALU_RESULT = 7,
  UNIT_MEMORY_IMMEDIATE = 8,
  UNIT_MEMORY_OPERAND = 9,
  UNIT_PC = 10,
  UNIT_ABS_IMMEDIATE = 11,
  UNIT_ABS_OPERAND = 12,
  UNIT_REGISTER_POINTER = 13,
};

class Instr;
using Program = std::vector<Instr>;
class Instr {
 public:
  std::vector<uint32_t> assemble() const;

  bool UsesSoperand() const;
  bool UsesDoperand() const;

  Instr& Src(Unit u);
  Instr& Dst(Unit u);
  Instr& Si(short i);
  Instr& Di(short i);

  Instr& Soperand(uint32_t o);

  Instr& Doperand(uint32_t o);

 private:
  struct OpFormat {
    unsigned short src_unit : 4;
    unsigned short si : 12;
    unsigned dst_unit : 4;
    unsigned short di : 12;
  };
  OpFormat op_;
  std::optional<uint32_t> soperand_;
  std::optional<uint32_t> doperand_;
};
