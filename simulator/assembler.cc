#include "assembler.h"

namespace {

bool NeedsOperand(Unit u) {
  switch (u) {
    case Unit::UNIT_NONE:
    case Unit::UNIT_STACK_PUSH_POP:
    case Unit::UNIT_STACK_INDEX:
    case Unit::UNIT_REGISTER:
    case Unit::UNIT_ALU_LEFT:
    case Unit::UNIT_ALU_RIGHT:
    case Unit::UNIT_ALU_OPERATOR:
    case Unit::UNIT_ALU_RESULT:
    case Unit::UNIT_MEMORY_IMMEDIATE:
    case Unit::UNIT_PC:
    case Unit::UNIT_ABS_IMMEDIATE:
      return false;
    case Unit::UNIT_MEMORY_OPERAND:
    case Unit::UNIT_ABS_OPERAND:
      return true;
  }
  return false;
}
}  // namespace

std::vector<uint32_t> Instr::assemble() const {
  CHECK_EQ(UsesSoperand(), soperand_.has_value());
  CHECK_EQ(UsesDoperand(), doperand_.has_value());

  std::vector<uint32_t> prg{*reinterpret_cast<const uint32_t*>(&op_)};
  if (UsesSoperand())
    prg.emplace_back(soperand_.value());
  if (UsesDoperand())
    prg.emplace_back(doperand_.value());

  return std::move(prg);
}

bool Instr::UsesSoperand() const {
  return NeedsOperand((Unit)op_.src_unit);
}

bool Instr::UsesDoperand() const {
  return NeedsOperand((Unit)op_.dst_unit);
}

Instr& Instr::Src(Unit u) {
  op_.src_unit = (unsigned short)u;
  DCHECK(op_.src_unit == (unsigned short)u);
  return *this;
}

Instr& Instr::Dst(Unit u) {
  op_.dst_unit = (unsigned short)u;
  DCHECK(op_.dst_unit == (unsigned short)u);
  return *this;
}

Instr& Instr::Si(const short i) {
  DCHECK(i < 1U << 12U);
  op_.si = i;
  return *this;
}

Instr& Instr::Di(const short i) {
  DCHECK(i < 1U << 12U);
  op_.di = i;
  return *this;
}

Instr& Instr::Soperand(uint32_t o) {
  CHECK(UsesSoperand());
  soperand_ = o;
  return *this;
}
Instr& Instr::Doperand(uint32_t o) {
  CHECK(UsesDoperand());
  doperand_ = o;
  return *this;
}
