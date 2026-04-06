// Fetch state machine for variable-length instructions (word-addressed).
//
// Instructions are 1, 2, or 3 words long:
//   Word 0: opcode (always present)
//   Word 1: source operand  (present when src unit is MEMORY_OPERAND or ABS_OPERAND)
//   Word 2: destination operand (present when dst unit is MEMORY_OPERAND or ABS_OPERAND)
//
// The sequencer reads the opcode, lets the decoder determine whether extra
// operand words are needed, fetches them if so, then asserts done_o. The
// program counter (pc_o) is advanced past all consumed words before done_o
// goes high. tta.sv holds the sequencer paused (via sel_i) until the execute
// stage finishes the current instruction.
module sequencer (
    input wire clk_i,                   // System clock
    input wire rst_i,                   // Synchronous reset (active high)
    bus_if.master instr_bus,            // Instruction fetch bus
    output logic [31:0] pc_o,           // Current program counter (word address)
    output logic [31:0] op_o,           // Fetched opcode word for the decoder
    output logic [31:0] src_operand_o,  // 32-bit source operand (when needed)
    output logic [31:0] dst_operand_o,  // 32-bit destination operand (when needed)

    input  logic need_src_operand_i,    // From decoder: source needs an operand word
    input  logic need_dst_operand_i,    // From decoder: destination needs an operand word
    input  logic sel_i,                 // Enable — held low to pause the sequencer
    output wire  decoder_enable_o,      // One-cycle pulse that gates the decoder

    output logic done_o                 // High when the full instruction is fetched
);
  // Fetch FSM states. The sequencer walks through START → READ_OPCODE →
  // DECODE, then optionally READ_SRC/DST_OPERAND before returning to START.
  enum {
    SEQ_START,                  // Issue bus read for opcode at pc_o
    SEQ_READ_OPCODE,            // Wait for bus ready, latch opcode
    SEQ_DECODE,                 // Let decoder extract fields; decide on operands
    SEQ_EXEC_SOURCE,            // (unused — reserved)
    SEQ_EXEC_DEST,              // (unused — reserved)
    SEQ_READ_SRC_OPERAND_START, // (unused — inlined into SEQ_DECODE)
    SEQ_READ_SRC_OPERAND,       // Wait for source operand word from bus
    SEQ_READ_DST_OPERAND_START, // Issue bus read for destination operand word
    SEQ_READ_DST_OPERAND        // Wait for destination operand word from bus
  } sequencer_state;

  // The decoder samples op_o only during the dedicated decode state.
  assign decoder_enable_o = sequencer_state == SEQ_DECODE;

  always @(posedge clk_i) begin
    if (rst_i) begin
      pc_o = 32'b0;
      op_o = 32'b0;
      sequencer_state = SEQ_START;
      instr_bus.valid = 1'b0;
    end else if (sel_i) begin
      case (sequencer_state)
        SEQ_START: begin
          // Begin reading the next opcode at the current program counter.
          instr_bus.valid = 1'b1;
          instr_bus.instr = 1'b1;
          instr_bus.addr = pc_o;
          sequencer_state = SEQ_READ_OPCODE;
          done_o = 1'b0;
        end
        SEQ_READ_OPCODE: begin
          if (instr_bus.ready) begin
            op_o = instr_bus.read_data;
            sequencer_state = SEQ_DECODE;
          end
        end
        SEQ_DECODE: begin
          // Extension operands, when needed, live in subsequent program words
          // at pc+1 (source) and pc+1 or pc+2 (destination).
          if (need_src_operand_i || need_dst_operand_i) begin
            instr_bus.valid = 1'b1;
            instr_bus.instr = 1'b0;
            instr_bus.addr  = pc_o + 1;
            if (need_src_operand_i) sequencer_state = SEQ_READ_SRC_OPERAND;
            else sequencer_state = SEQ_READ_DST_OPERAND;
          end else begin
            // No operand words — instruction is 1 word. Advance PC past it.
            done_o = 1'b1;
            pc_o = pc_o + 1;
            sequencer_state = SEQ_START;
          end
        end
        SEQ_READ_SRC_OPERAND: begin
          if (instr_bus.ready) begin
            src_operand_o = instr_bus.read_data;
            // Advance past the source operand word.
            pc_o = pc_o + 1;
            if (need_dst_operand_i) sequencer_state = SEQ_READ_DST_OPERAND_START;
            else begin
              // 2-word instruction (opcode + src operand). Advance past opcode.
              done_o = 1'b1;
              pc_o = pc_o + 1;
              sequencer_state = SEQ_START;
            end
          end
        end
        SEQ_READ_DST_OPERAND_START: begin
          // Issue a bus read for the destination operand at pc+1.
          instr_bus.addr  = pc_o + 1;
          instr_bus.valid = 1'b1;
          instr_bus.instr = 1'b0;
          sequencer_state = SEQ_READ_DST_OPERAND;
        end
        SEQ_READ_DST_OPERAND: begin
          if (instr_bus.ready) begin
            dst_operand_o = instr_bus.read_data;
            // 3-word instruction: advance past dst operand + opcode.
            pc_o = pc_o + 2;
            done_o = 1'b1;
            sequencer_state = SEQ_START;
          end
        end
      endcase
    end
  end

endmodule : sequencer
