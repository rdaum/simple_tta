#include <glog/logging.h>
#include <gtest/gtest.h>
#include <verilated_fst_c.h>

#include <memory>

#include "Vtesttop.h"
#include "assembler.h"
#include "clock_gen.h"
#include "ram_sim.h"

// A kind of integration tests that runs through some common
// operations and checks their results.
// Not exhaustive yet.

// TODO: unit tests which run against the individual components
// (Execute/Decode/Sequencer etc) rather than the top level.

class TTATest : public ::testing::Test {
 public:
  TTATest()
      : top_(std::make_unique<Vtesttop>()),
        clock_gen_(1, 1 /* reset_cycles */, &top_->rst_i, &top_->sysclk_i),
        prg_(1024,
             c_gnd_,
             top_->instr_valid_o,
             &top_->instr_ready_i,
             &top_->instr_data_read_i,
             i_gnd_,
             top_->instr_addr_o),
        ram_(1024,
             top_->data_wstrb_o,
             top_->data_valid_o,
             &top_->data_ready_i,
             &top_->data_data_read_i,
             top_->data_data_write_o,
             top_->data_addr_o) {}

 protected:
  void SetUp() override {
    Reset();
    Verilated::traceEverOn(true);
    std::string trace_name = ::testing::UnitTest::GetInstance()
                                 ->current_test_info()
                                 ->test_case_name();
    trace_name.push_back('-');
    trace_name.append(
        ::testing::UnitTest::GetInstance()->current_test_info()->name());
    trace_name.append(".vcd");
    trace_ = std::make_unique<VerilatedFstC>();
    top_->trace(trace_.get(), 99);
    trace_->open(trace_name.c_str());
  }

  void TearDown() override {
    trace_->flush();
    trace_->close();
  }

 public:
  void Reset() { top_->rst_i = 1; }

  void Step() {
    clock_gen_.Step(trace_.get());
    top_->eval();
    if (!top_->rst_i & clock_gen_.Bus()) {
      ram_.Do();
      prg_.Do();
    }
  }

  /*
   * Run until "pin" equals "val" or max_clocks has been reached.
   * Returns true if the pin reached the intended value before the clock ran
   * out.
   */
  template <typename T>
  bool RunUntil(T* pin, T val, int max_clocks) {
    int start_clocks = clock_gen_.cycles();
    while (!Verilated::gotFinish()) {
      Step();

      if (*pin == val || clock_gen_.cycles())
        return true;
      else if (clock_gen_.cycles() - start_clocks >= max_clocks)
        return false;
    }
    return false;
  }

  /**
   * Run until max_clocks cycles have executed.
   */
  int RunUntil(int max_clocks) {
    int start_clk = clock_gen_.cycles();
    while (!Verilated::gotFinish() &&
           (clock_gen_.cycles() < max_clocks + start_clk)) {
      Step();
    }
    return clock_gen_.cycles() - start_clk;
  }

  void Load(const Program& program, uint32_t addr = 0) {
    off_t pos = addr;
    for (auto& instr : program) {
      std::vector<uint32_t> code = instr.assemble();
      for (const auto& op : code) {
        prg_.mem()[pos++] = op;
      }
    }
  }

 protected:
  std::unique_ptr<VerilatedFstC> trace_;

  Vtesttop* top() const { return top_.get(); }
  const ClockGenerator& clk() const { return clock_gen_; }
  RAMSim* ram() { return &ram_; }
  RAMSim* prg() { return &prg_; }

 private:
  std::unique_ptr<Vtesttop> top_;
  ClockGenerator clock_gen_;
  RAMSim prg_;
  RAMSim ram_;

  CData c_gnd_ = 0;
  IData i_gnd_ = 0;
};

TEST_F(TTATest, Initialize) {
  ASSERT_TRUE(RunUntil(&top()->rst_i, (CData)1, 1));
}


// Test absolute immediate value into register, then into immediate memory
// address
TEST_F(TTATest, RegisterSetAbsMemorySetAbs) {
  Load({Instr()
            .Src(Unit::UNIT_ABS_IMMEDIATE)
            .Si(666)
            .Dst(Unit::UNIT_REGISTER)
            .Di(0),
        Instr()
            .Src(Unit::UNIT_REGISTER)
            .Si(0)
            .Dst(Unit::UNIT_MEMORY_IMMEDIATE)
            .Di(123)});
  ASSERT_TRUE(RunUntil(&top()->rst_i, (CData)1, 1));  // Clear the reset

  EXPECT_EQ(RunUntil(8), 8); /* no more than 8 clocks used */

  EXPECT_EQ(top()->rst_i, 0);
  EXPECT_EQ(ram()->mem()[123], 666);
}

TEST_F(TTATest, MemImmediateToMemImmediate) {
  Load({Instr()
            .Src(Unit::UNIT_MEMORY_IMMEDIATE)
            .Si(123)
            .Dst(Unit::UNIT_MEMORY_IMMEDIATE)
            .Di(124)});
  ASSERT_TRUE(RunUntil(&top()->rst_i, (CData)1, 1));  // Clear the reset
  ram()->mem()[123] = 666;
  RunUntil(25);
  EXPECT_EQ(ram()->mem()[124], 666);
}

TEST_F(TTATest, MemOperandToMemOperand) {
  Load({Instr()
            .Src(Unit::UNIT_MEMORY_OPERAND)
            .Soperand(123)
            .Dst(Unit::UNIT_MEMORY_OPERAND)
            .Doperand(124)});
  ASSERT_TRUE(RunUntil(&top()->rst_i, (CData)1, 1));  // Clear the reset
  ram()->mem()[123] = 666;
  RunUntil(25);
  EXPECT_EQ(ram()->mem()[124], 666);
}

TEST_F(TTATest, PointerValToMemImmediate) {
  Load({
    Instr().Src(Unit::UNIT_ABS_IMMEDIATE).Si(666).Dst(Unit::UNIT_MEMORY_IMMEDIATE).Di(123),
    Instr().Src(Unit::UNIT_ABS_IMMEDIATE).Si(123).Dst(Unit::UNIT_REGISTER).Di(1),
    Instr().Src(Unit::UNIT_REGISTER_POINTER).Si(1).Dst(Unit::UNIT_MEMORY_IMMEDIATE).Di(124)
  });
  ASSERT_TRUE(RunUntil(&top()->rst_i, (CData)1, 1));  // Clear the reset
  RunUntil(100);
  EXPECT_EQ(ram()->mem()[124], 666);
}

TEST_F(TTATest, MemOperandToRegisterToMemoryOperand) {
  Load({Instr()
            .Src(Unit::UNIT_MEMORY_OPERAND)
            .Soperand(123)
            .Dst(Unit::UNIT_REGISTER)
            .Di(0),
        Instr()
            .Src(Unit::UNIT_REGISTER)
            .Si(0)
            .Dst(Unit::UNIT_MEMORY_OPERAND)
            .Doperand(124)});
  ASSERT_TRUE(RunUntil(&top()->rst_i, (CData)1, 1));  // Clear the reset
  ram()->mem()[123] = 666;
  RunUntil(25);
  EXPECT_EQ(ram()->mem()[124], 666);
}

// Test addition source absolute values, destination memory
TEST_F(TTATest, AluAddition) {
  Load({Instr()
            .Src(Unit::UNIT_ABS_IMMEDIATE)
            .Si(666)
            .Dst(Unit::UNIT_ALU_LEFT)
            .Di(0),
        Instr()
            .Src(Unit::UNIT_ABS_IMMEDIATE)
            .Si(111)
            .Dst(Unit::UNIT_ALU_RIGHT)
            .Di(0),
        Instr()
            .Src(Unit::UNIT_ABS_IMMEDIATE)
            .Si((int)ALUOp::ALU_ADD)
            .Dst(Unit::UNIT_ALU_OPERATOR)
            .Di(0),
        Instr()
            .Src(Unit::UNIT_ALU_RESULT)
            .Si(0)
            .Dst(Unit::UNIT_MEMORY_IMMEDIATE)
            .Di(123)});
  ASSERT_TRUE(RunUntil(&top()->rst_i, (CData)1, 1));  // Clear the reset

  EXPECT_EQ(RunUntil(17), 17); /* no more than 16 clocks used */

  EXPECT_TRUE(top()->instr_done_o);
  EXPECT_EQ(top()->rst_i, 0);
  EXPECT_EQ(ram()->mem()[123], 777);
}

// TODO: set/get PC, stack, other ALU ops