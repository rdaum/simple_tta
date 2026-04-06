// Fetch state machine for variable-length instructions (word-addressed).
//
// Instructions are 1, 2, or 3 words long:
//   Word 0: opcode (always present)
//   Word 1: source operand  (present when src unit is MEMORY_OPERAND or ABS_OPERAND)
//   Word 2: destination operand (present when dst unit is MEMORY_OPERAND or ABS_OPERAND)
//
// Architecture: three concerns, cleanly separated:
//   1. Fetch FSM: reads instruction words from instr_bus into the
//      prefetch buffer. Runs unconditionally (overlaps with execute).
//   2. Prefetch buffer + valid flag: holds the complete fetched
//      instruction until execute is ready.
//   3. Handoff: when prefetch_valid && !exec_busy_i, promotes the
//      buffer to the decoder-facing outputs and pulses done_o.
//      This happens in exactly one place.
//
// On a taken branch, pc_write_en_i clears prefetch_valid and restarts
// the fetch FSM from the new PC.
module sequencer (
    input wire clk_i,                   // System clock
    input wire rst_i,                   // Synchronous reset (active high)
    bus_if.master instr_bus,            // Instruction fetch bus
    output logic [31:0] pc_o,           // Current program counter (word address)
    output logic [31:0] op_o,           // Fetched opcode word for the decoder
    output logic [31:0] src_operand_o,  // 32-bit source operand (when needed)
    output logic [31:0] dst_operand_o,  // 32-bit destination operand (when needed)

    input  logic        exec_busy_i,    // High while execute is processing

    // PC override from execute (for jumps / conditional branches).
    input  logic [31:0] pc_write_i,     // New PC value
    input  logic        pc_write_en_i,  // High to override PC with pc_write_i

    output logic done_o                 // Pulses high when instruction is ready
);

  // --- Fetch FSM state ---
  enum {
    SEQ_FETCH_START,            // Issue bus read for opcode at pc_o
    SEQ_FETCH_OPCODE,           // Wait for bus ready, latch opcode
    SEQ_FETCH_SRC_OPERAND,      // Wait for source operand word from bus
    SEQ_FETCH_DST_OPERAND_SETUP,// Issue bus read for destination operand
    SEQ_FETCH_DST_OPERAND,      // Wait for destination operand from bus
    SEQ_FETCH_IDLE              // Fetch complete, waiting for handoff
  } fetch_state;

  // --- Prefetch buffer ---
  logic        prefetch_valid;
  logic [31:0] prefetch_op;
  logic [31:0] prefetch_src_operand;
  logic [31:0] prefetch_dst_operand;

  // --- Helpers ---
  function automatic logic needs_src_op(logic [31:0] raw_op);
    Unit su = Unit'(raw_op[3:0]);
    return su == UNIT_MEMORY_OPERAND || su == UNIT_ABS_OPERAND;
  endfunction

  function automatic logic needs_dst_op(logic [31:0] raw_op);
    Unit du = Unit'(raw_op[19:16]);
    return du == UNIT_MEMORY_OPERAND || du == UNIT_ABS_OPERAND;
  endfunction

  wire [31:0] pc_plus_1 = pc_o + 1;
  wire [31:0] pc_plus_2 = pc_o + 2;

  always @(posedge clk_i) begin
    if (rst_i) begin
      pc_o <= 32'b0;
      op_o <= 32'b0;
      src_operand_o <= 32'b0;
      dst_operand_o <= 32'b0;
      prefetch_valid <= 1'b0;
      prefetch_op <= 32'b0;
      prefetch_src_operand <= 32'b0;
      prefetch_dst_operand <= 32'b0;
      done_o <= 1'b0;
      fetch_state <= SEQ_FETCH_START;
      instr_bus.valid <= 1'b0;
      instr_bus.instr <= 1'b0;
      instr_bus.addr <= 32'b0;
    end else if (pc_write_en_i) begin
      // Branch taken: discard prefetch, restart fetch from new PC.
      pc_o <= pc_write_i;
      prefetch_valid <= 1'b0;
      instr_bus.valid <= 1'b0;
      done_o <= 1'b0;
      fetch_state <= SEQ_FETCH_START;
    end else begin

      // === Handoff: promote prefetch buffer to outputs ===
      // This is the ONLY place done_o is asserted and outputs are updated.
      if (done_o) begin
        done_o <= 1'b0;
      end else if (prefetch_valid && !exec_busy_i) begin
        op_o <= prefetch_op;
        src_operand_o <= prefetch_src_operand;
        dst_operand_o <= prefetch_dst_operand;
        done_o <= 1'b1;
        prefetch_valid <= 1'b0;
        // Kick the fetch FSM to start the next instruction.
        // (If it's already in FETCH_IDLE, it will advance on the
        // next cycle. If it finished fetching while we were waiting,
        // it's already idle.)
        if (fetch_state == SEQ_FETCH_IDLE)
          fetch_state <= SEQ_FETCH_START;
      end

      // === Fetch FSM: runs unconditionally (prefetch) ===
      case (fetch_state)
        SEQ_FETCH_START: begin
          if (!prefetch_valid) begin
            // Only start a new fetch if the prefetch buffer is empty.
            instr_bus.valid <= 1'b1;
            instr_bus.instr <= 1'b1;
            instr_bus.addr <= pc_o;
            fetch_state <= SEQ_FETCH_OPCODE;
          end
        end

        SEQ_FETCH_OPCODE: begin
          if (instr_bus.ready) begin
            prefetch_op <= instr_bus.read_data;
            if (needs_src_op(instr_bus.read_data) || needs_dst_op(instr_bus.read_data)) begin
              instr_bus.valid <= 1'b1;
              instr_bus.instr <= 1'b0;
              instr_bus.addr  <= pc_plus_1;
              if (needs_src_op(instr_bus.read_data))
                fetch_state <= SEQ_FETCH_SRC_OPERAND;
              else
                fetch_state <= SEQ_FETCH_DST_OPERAND;
            end else begin
              // 1-word instruction complete.
              pc_o <= pc_plus_1;
              instr_bus.valid <= 1'b0;
              prefetch_valid <= 1'b1;
              fetch_state <= SEQ_FETCH_IDLE;
            end
          end
        end

        SEQ_FETCH_SRC_OPERAND: begin
          if (instr_bus.ready) begin
            prefetch_src_operand <= instr_bus.read_data;
            if (needs_dst_op(prefetch_op)) begin
              // 3-word instruction: still need dst operand.
              instr_bus.valid <= 1'b0;
              pc_o <= pc_plus_1;
              fetch_state <= SEQ_FETCH_DST_OPERAND_SETUP;
            end else begin
              // 2-word instruction complete.
              pc_o <= pc_plus_2;
              instr_bus.valid <= 1'b0;
              prefetch_valid <= 1'b1;
              fetch_state <= SEQ_FETCH_IDLE;
            end
          end
        end

        SEQ_FETCH_DST_OPERAND_SETUP: begin
          instr_bus.addr  <= pc_plus_1;
          instr_bus.valid <= 1'b1;
          instr_bus.instr <= 1'b0;
          fetch_state <= SEQ_FETCH_DST_OPERAND;
        end

        SEQ_FETCH_DST_OPERAND: begin
          if (instr_bus.ready) begin
            prefetch_dst_operand <= instr_bus.read_data;
            // 3-word instruction complete.
            pc_o <= pc_plus_2;
            instr_bus.valid <= 1'b0;
            prefetch_valid <= 1'b1;
            fetch_state <= SEQ_FETCH_IDLE;
          end
        end

        SEQ_FETCH_IDLE: begin
          // Waiting for handoff to clear prefetch_valid.
          // The handoff block above will set fetch_state <= SEQ_FETCH_START
          // when it promotes the buffer.
        end
      endcase
    end
  end

endmodule : sequencer
