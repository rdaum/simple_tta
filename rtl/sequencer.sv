// Fetch state machine for variable-length instructions (word-addressed).
//
// Instructions are 1, 2, or 3 words long:
//   Word 0: opcode (always present)
//   Word 1: source operand  (present when src unit is MEMORY_OPERAND or ABS_OPERAND)
//   Word 2: destination operand (present when dst unit is MEMORY_OPERAND or ABS_OPERAND)
//
// Architecture: three concerns, cleanly separated:
//   1. Fetch FSM: reads instruction words from instr_bus into the
//      instruction queue. Runs whenever the queue has a free slot.
//   2. Instruction queue: a 2-entry FIFO of complete instructions.
//      Each entry holds {pc, op, src_operand, dst_operand}.
//   3. Handoff: when instr_accept_i fires, dequeues the head entry
//      and promotes it to the decoder-facing outputs. This happens
//      in exactly one place.
//
// The valid/accept contract:
//   - instr_valid_o is high whenever the queue is non-empty.
//   - instr_accept_i is a combinational signal from execute, high
//     for exactly one cycle when execute consumes the instruction.
//   - On accept, the sequencer promotes the head entry to outputs
//     and advances the read pointer.
//
// pc_o semantics: pc_o is the PC value associated with the instruction
// currently presented on op_o. It equals (instruction_address +
// instruction_word_count), i.e., the address of the next sequential
// instruction. This is what execute sees via UNIT_PC.
//
// Fetch policy: the fetch FSM will NOT start fetching past a control-
// flow instruction (UNIT_PC or UNIT_PC_COND as destination). It stalls
// until execute accepts the branch, at which point either a flush
// occurs (taken) or sequential fetch resumes (not taken). This means
// the queue never contains wrong-path instructions.
//
// On a taken branch, pc_write_en_i invalidates the entire queue and
// restarts the fetch FSM from the new PC.
module sequencer (
    input wire clk_i,                   // System clock
    input wire rst_i,                   // Synchronous reset (active high)

    // Instruction fetch bus (explicit ports replacing bus_if.master)
    output logic [3:0]  instr_wstrb_o,
    output logic [31:0] instr_write_data_o,
    output logic [31:0] instr_addr_o,
    output logic        instr_valid_o_bus,
    output logic        instr_instr_o,
    input  logic        instr_ready_i,
    input  logic [31:0] instr_read_data_i,
    output logic [31:0] pc_o,           // PC of current instruction (see above)
    output logic [31:0] op_o,           // Fetched opcode word for the decoder
    output logic [31:0] src_operand_o,  // 32-bit source operand (when needed)
    output logic [31:0] dst_operand_o,  // 32-bit destination operand (when needed)

    // Valid/accept handshake with execute.
    output wire         instr_valid_o,  // Queue has a complete instruction
    input  wire         instr_accept_i, // Execute is consuming the instruction this cycle

    // PC override from execute (for jumps / conditional branches).
    input  logic [31:0] pc_write_i,     // New PC value
    input  logic        pc_write_en_i   // High to override PC with pc_write_i

`ifdef SEQUENCER_DEBUG
    ,output wire        dbg_prefetch_valid_o,
    output wire  [2:0]  dbg_fetch_state_o,
    output wire  [31:0] dbg_prefetch_op_o
`endif
);

  // --- Fetch FSM state ---
  enum {
    SEQ_FETCH_START,
    SEQ_FETCH_OPCODE,
    SEQ_FETCH_SRC_OPERAND,
    SEQ_FETCH_DST_OPERAND_SETUP,
    SEQ_FETCH_DST_OPERAND,
    SEQ_FETCH_IDLE
  } fetch_state;

  // --- Instruction queue (2-entry FIFO) ---
  localparam QUEUE_DEPTH = 2;

  // Each entry: {pc, op, src_operand, dst_operand}
  logic [31:0] q_pc          [QUEUE_DEPTH-1:0];
  logic [31:0] q_op          [QUEUE_DEPTH-1:0];
  logic [31:0] q_src_operand [QUEUE_DEPTH-1:0];
  logic [31:0] q_dst_operand [QUEUE_DEPTH-1:0];
  logic        q_valid       [QUEUE_DEPTH-1:0];

  // Queue pointers (1-bit each for a 2-entry queue).
  logic wr_ptr, rd_ptr;
  wire  queue_full  = q_valid[0] && q_valid[1];
  wire  queue_has_space = !queue_full;

  // The head of the queue is the entry execute will consume.
  assign instr_valid_o = q_valid[rd_ptr];

  // Decoder-facing outputs: combinational mux so that on the accept
  // cycle, execute sees the new instruction's fields immediately
  // (no 1-cycle delay from non-blocking promotion). When accept is
  // not active, the registered values hold the previous instruction
  // stable for multi-cycle execute.
  logic [31:0] reg_op, reg_src_operand, reg_dst_operand, reg_pc;
  assign op_o          = instr_accept_i ? q_op[rd_ptr]          : reg_op;
  assign src_operand_o = instr_accept_i ? q_src_operand[rd_ptr] : reg_src_operand;
  assign dst_operand_o = instr_accept_i ? q_dst_operand[rd_ptr] : reg_dst_operand;
  assign pc_o          = instr_accept_i ? q_pc[rd_ptr]          : reg_pc;

  // Staging area: the fetch FSM builds the instruction here before
  // enqueueing it as a complete entry.
  logic [31:0] staging_op;
  logic [31:0] staging_src_operand;

  // Fetch address — separate from pc_o. Advances as the fetch FSM
  // reads instruction words. pc_o is only updated on handoff.
  logic [31:0] fetch_pc;

`ifdef SEQUENCER_DEBUG
  assign dbg_prefetch_valid_o = instr_valid_o;
  assign dbg_fetch_state_o = fetch_state[2:0];
  assign dbg_prefetch_op_o = staging_op;
`endif

  // Fetch is blocked when the queue contains an unresolved control-flow
  // instruction (UNIT_PC or UNIT_PC_COND as destination). Cleared when
  // execute accepts the branch instruction — either a flush follows
  // (taken) or sequential execution continues (not taken).
  logic fetch_stalled_on_branch;

  // --- Helpers ---
  /* verilator lint_off UNUSEDSIGNAL */
  function automatic needs_src_op;
    input [31:0] raw_op;
    needs_src_op = (raw_op[4:0] == UNIT_MEMORY_OPERAND) || (raw_op[4:0] == UNIT_ABS_OPERAND);
  endfunction

  function automatic needs_dst_op;
    input [31:0] raw_op;
    needs_dst_op = (raw_op[9:5] == UNIT_MEMORY_OPERAND) || (raw_op[9:5] == UNIT_ABS_OPERAND);
  endfunction

  function automatic is_control_flow;
    input [31:0] raw_op;
    is_control_flow = (raw_op[9:5] == UNIT_PC) || (raw_op[9:5] == UNIT_PC_COND);
  endfunction
  /* verilator lint_on UNUSEDSIGNAL */

  wire [31:0] fetch_pc_plus_1 = fetch_pc + 1;
  wire [31:0] fetch_pc_plus_2 = fetch_pc + 2;

  always @(posedge clk_i) begin
    if (rst_i) begin
      reg_pc <= 32'b0;
      reg_op <= 32'b0;
      reg_src_operand <= 32'b0;
      reg_dst_operand <= 32'b0;
      fetch_pc <= 32'b0;
      staging_op <= 32'b0;
      staging_src_operand <= 32'b0;
      q_valid[0] <= 1'b0;
      q_valid[1] <= 1'b0;
      wr_ptr <= 1'b0;
      rd_ptr <= 1'b0;
      fetch_stalled_on_branch <= 1'b0;
      fetch_state <= SEQ_FETCH_START;
      instr_valid_o_bus <= 1'b0;
      instr_instr_o <= 1'b0;
      instr_addr_o <= 32'b0;
      instr_wstrb_o <= 4'b0;
      instr_write_data_o <= 32'b0;
    end else if (pc_write_en_i) begin
      // Branch taken: flush entire queue, restart fetch from new PC.
      fetch_pc <= pc_write_i;
      q_valid[0] <= 1'b0;
      q_valid[1] <= 1'b0;
      wr_ptr <= 1'b0;
      rd_ptr <= 1'b0;
      fetch_stalled_on_branch <= 1'b0;
      instr_valid_o_bus <= 1'b0;
      fetch_state <= SEQ_FETCH_START;
    end else begin

      // === Handoff: dequeue head entry ===
      // The decoder-facing outputs (op_o, etc.) are combinational muxes
      // that show the queue head on the accept cycle. Here we latch the
      // values into reg_* so they remain stable for multi-cycle execute.
      if (instr_accept_i && q_valid[rd_ptr]) begin
        reg_op <= q_op[rd_ptr];
        reg_src_operand <= q_src_operand[rd_ptr];
        reg_dst_operand <= q_dst_operand[rd_ptr];
        reg_pc <= q_pc[rd_ptr];
        // If the accepted instruction was the branch that stalled us,
        // clear the stall — execute will either flush (taken) or
        // sequential fetch is now safe (not taken).
        fetch_stalled_on_branch <= 1'b0;
        q_valid[rd_ptr] <= 1'b0;
        rd_ptr <= ~rd_ptr;
      end

      // === Fetch FSM: runs when queue has space and no unresolved branch ===
      case (fetch_state)
        SEQ_FETCH_START: begin
          if ((queue_has_space || instr_accept_i) && !fetch_stalled_on_branch) begin
            instr_valid_o_bus <= 1'b1;
            instr_instr_o <= 1'b1;
            instr_addr_o <= fetch_pc;
            fetch_state <= SEQ_FETCH_OPCODE;
          end
        end

        SEQ_FETCH_OPCODE: begin
          if (instr_ready_i) begin
            staging_op <= instr_read_data_i;
            if (needs_src_op(instr_read_data_i) || needs_dst_op(instr_read_data_i)) begin
              instr_valid_o_bus <= 1'b1;
              instr_instr_o <= 1'b0;
              instr_addr_o  <= fetch_pc_plus_1;
              if (needs_src_op(instr_read_data_i))
                fetch_state <= SEQ_FETCH_SRC_OPERAND;
              else
                fetch_state <= SEQ_FETCH_DST_OPERAND;
            end else begin
              // 1-word instruction complete. Enqueue it.
              q_op[wr_ptr] <= instr_read_data_i;
              q_src_operand[wr_ptr] <= 32'b0;
              q_dst_operand[wr_ptr] <= 32'b0;
              q_pc[wr_ptr] <= fetch_pc_plus_1;
              q_valid[wr_ptr] <= 1'b1;
              wr_ptr <= ~wr_ptr;
              fetch_pc <= fetch_pc_plus_1;
              instr_valid_o_bus <= 1'b0;
              if (is_control_flow(instr_read_data_i))
                fetch_stalled_on_branch <= 1'b1;
              fetch_state <= SEQ_FETCH_START;
            end
          end
        end

        SEQ_FETCH_SRC_OPERAND: begin
          if (instr_ready_i) begin
            staging_src_operand <= instr_read_data_i;
            if (needs_dst_op(staging_op)) begin
              // 3-word instruction: still need dst operand.
              instr_valid_o_bus <= 1'b0;
              fetch_pc <= fetch_pc_plus_1;
              fetch_state <= SEQ_FETCH_DST_OPERAND_SETUP;
            end else begin
              // 2-word instruction complete. Enqueue it.
              q_op[wr_ptr] <= staging_op;
              q_src_operand[wr_ptr] <= instr_read_data_i;
              q_dst_operand[wr_ptr] <= 32'b0;
              q_pc[wr_ptr] <= fetch_pc_plus_2;
              q_valid[wr_ptr] <= 1'b1;
              wr_ptr <= ~wr_ptr;
              fetch_pc <= fetch_pc_plus_2;
              instr_valid_o_bus <= 1'b0;
              if (is_control_flow(staging_op))
                fetch_stalled_on_branch <= 1'b1;
              fetch_state <= SEQ_FETCH_START;
            end
          end
        end

        SEQ_FETCH_DST_OPERAND_SETUP: begin
          instr_addr_o  <= fetch_pc_plus_1;
          instr_valid_o_bus <= 1'b1;
          instr_instr_o <= 1'b0;
          fetch_state <= SEQ_FETCH_DST_OPERAND;
        end

        SEQ_FETCH_DST_OPERAND: begin
          if (instr_ready_i) begin
            // 3-word instruction complete. Enqueue it.
            q_op[wr_ptr] <= staging_op;
            q_src_operand[wr_ptr] <= staging_src_operand;
            q_dst_operand[wr_ptr] <= instr_read_data_i;
            q_pc[wr_ptr] <= fetch_pc_plus_2;
            q_valid[wr_ptr] <= 1'b1;
            wr_ptr <= ~wr_ptr;
            fetch_pc <= fetch_pc_plus_2;
            instr_valid_o_bus <= 1'b0;
            if (is_control_flow(staging_op))
              fetch_stalled_on_branch <= 1'b1;
            fetch_state <= SEQ_FETCH_START;
          end
        end

        SEQ_FETCH_IDLE: begin
          // Not used with queue — fetch FSM goes directly to START
          // when there's space. Kept for enum completeness.
        end
      endcase
    end
  end

endmodule
