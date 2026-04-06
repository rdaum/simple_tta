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

    input  logic sel_i,                 // Enable — held low to pause the sequencer

    // PC override from execute (for jumps / conditional branches).
    // Sampled when done_o is about to go high on the next fetch cycle.
    input  logic [31:0] pc_write_i,     // New PC value
    input  logic        pc_write_en_i,  // High to override PC with pc_write_i

    output logic done_o                 // High when the full instruction is fetched
);
  // Fetch FSM states. The decoder is now combinational, so the sequencer
  // can check need_src/dst_operand in the same cycle it latches the opcode
  // (no separate DECODE state needed).
  enum {
    SEQ_START,                  // Issue bus read for opcode at pc_o
    SEQ_READ_OPCODE,            // Wait for bus ready, latch opcode + decide on operands
    SEQ_READ_SRC_OPERAND,       // Wait for source operand word from bus
    SEQ_READ_DST_OPERAND_START, // Issue bus read for destination operand word
    SEQ_READ_DST_OPERAND        // Wait for destination operand word from bus
  } sequencer_state;

  // Inline operand-needed checks so the sequencer can decide in the same
  // cycle it latches the opcode, without waiting for the external decoder.
  function automatic logic needs_src_op(logic [31:0] raw_op);
    Unit su = Unit'(raw_op[3:0]);
    return su == UNIT_MEMORY_OPERAND || su == UNIT_ABS_OPERAND;
  endfunction

  function automatic logic needs_dst_op(logic [31:0] raw_op);
    Unit du = Unit'(raw_op[19:16]);
    return du == UNIT_MEMORY_OPERAND || du == UNIT_ABS_OPERAND;
  endfunction

  always @(posedge clk_i) begin
    if (rst_i) begin
      pc_o <= 32'b0;
      op_o <= 32'b0;
      src_operand_o <= 32'b0;
      dst_operand_o <= 32'b0;
      done_o <= 1'b0;
      sequencer_state <= SEQ_START;
      instr_bus.valid <= 1'b0;
      instr_bus.instr <= 1'b0;
      instr_bus.addr <= 32'b0;
    end else if (sel_i) begin
      case (sequencer_state)
        SEQ_START: begin
          automatic logic [31:0] fetch_pc = pc_write_en_i ? pc_write_i : pc_o;
          // If execute requested a PC override (jump/branch), apply it now
          // before fetching the next opcode.
          pc_o <= fetch_pc;
          // Begin reading the next opcode at the current program counter.
          instr_bus.valid <= 1'b1;
          instr_bus.instr <= 1'b1;
          instr_bus.addr <= fetch_pc;
          sequencer_state <= SEQ_READ_OPCODE;
          done_o <= 1'b0;
        end
        SEQ_READ_OPCODE: begin
          if (instr_bus.ready) begin
            op_o <= instr_bus.read_data;
            // Determine operand requirements directly from the raw bus data,
            // since the combinational decoder may not have re-evaluated yet
            // within this always block.
            if (needs_src_op(instr_bus.read_data) || needs_dst_op(instr_bus.read_data)) begin
              instr_bus.valid <= 1'b1;
              instr_bus.instr <= 1'b0;
              instr_bus.addr  <= pc_o + 1;
              if (needs_src_op(instr_bus.read_data)) sequencer_state <= SEQ_READ_SRC_OPERAND;
              else sequencer_state <= SEQ_READ_DST_OPERAND;
            end else begin
              automatic logic [31:0] next_pc = pc_o + 1;
              // No operand words — instruction is 1 word. Advance PC past it.
              done_o <= 1'b1;
              pc_o <= next_pc;
              sequencer_state <= SEQ_START;
            end
          end
        end
        SEQ_READ_SRC_OPERAND: begin
          if (instr_bus.ready) begin
            src_operand_o <= instr_bus.read_data;
            // Advance past the source operand word.
            if (needs_dst_op(op_o)) begin
              pc_o <= pc_o + 1;
              sequencer_state <= SEQ_READ_DST_OPERAND_START;
            end
            else begin
              automatic logic [31:0] next_pc = pc_o + 2;
              // 2-word instruction (opcode + src operand). Advance past opcode.
              done_o <= 1'b1;
              pc_o <= next_pc;
              sequencer_state <= SEQ_START;
            end
          end
        end
        SEQ_READ_DST_OPERAND_START: begin
          // Issue a bus read for the destination operand at pc+1.
          instr_bus.addr  <= pc_o + 1;
          instr_bus.valid <= 1'b1;
          instr_bus.instr <= 1'b0;
          sequencer_state <= SEQ_READ_DST_OPERAND;
        end
        SEQ_READ_DST_OPERAND: begin
          if (instr_bus.ready) begin
            automatic logic [31:0] next_pc = pc_o + 2;
            dst_operand_o <= instr_bus.read_data;
            // 3-word instruction: advance past dst operand + opcode.
            pc_o <= next_pc;
            done_o <= 1'b1;
            sequencer_state <= SEQ_START;
          end
        end
      endcase
    end
  end

endmodule : sequencer
