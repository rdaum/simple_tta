`include "common.vh"

// Eight independent 64-word stacks used for push/pop plus indexed peek/poke.
// Offsets are measured from the top of stack: 0 = top element.
//
// Empty-ascending convention: each stack pointer (SP) points at the next
// free slot. Push stores at SP then increments; pop decrements SP then
// reads. Indexed access counts backward from the top element.
//
// The combinational read path (always_comb) provides pop/peek results
// on data_o in the same cycle the operation state is active. The
// sequential path (always_ff) updates stack memory and pointers on the
// following clock edge.
module stack_unit (
    input wire clk_i,
    input wire rst_i,

    // Stack control interface
    input wire [2:0] stack_select_i,     // 0-7 stack selection
    input wire stack_push_i,
    input wire stack_pop_i,
    input wire [5:0] stack_offset_i,     // For indexed access (0-63)
    input wire stack_index_read_i,
    input wire stack_index_write_i,

    // Data interface
    input wire [31:0] data_i,
    output logic [31:0] data_o,
    output logic stack_ready_o,

    // Error reporting
    output logic stack_overflow_o,
    output logic stack_underflow_o
);

  localparam STACK_DEPTH = 64;
  localparam STACK_PTR_BITS = 6;
  localparam [STACK_PTR_BITS-1:0] STACK_EMPTY = 6'd0;

  // 8 independent stacks, each 64 words deep.
  reg [31:0] stack_mem[0:7][0:STACK_DEPTH-1];

  // Stack pointers: each points at the next free slot (empty-ascending).
  reg [STACK_PTR_BITS-1:0] stack_pointers[0:7];

  // Per-stack error status.
  reg [7:0] overflow_status;
  reg [7:0] underflow_status;

  // FSM state.
  typedef enum logic [1:0] {
    STACK_IDLE,
    STACK_PUSHING,
    STACK_POPPING,
    STACK_INDEXING
  } stack_state_t;

  stack_state_t state;
  reg [2:0] active_stack;
  reg pending_write;

  // Write forwarding: captures the last write so that a read
  // immediately following a write to the same address sees the
  // new data (read-after-write hazard).
  reg [2:0] last_write_stack_id;
  reg [STACK_PTR_BITS-1:0] last_write_addr;
  reg [31:0] last_write_data;
  reg last_write_valid;

  // --- Combinational read path ---
  // Provides pop/peek results on data_o without an extra cycle.
  // Also computes the absolute index for indexed operations.
  logic [STACK_PTR_BITS-1:0] abs_index;

  always_comb begin
    abs_index = 6'b0;
    data_o = 32'b0;

    if (!rst_i) begin
      if (state == STACK_POPPING) begin
        if (stack_pointers[active_stack] != STACK_EMPTY) begin
          // Write forwarding check.
          if (last_write_valid &&
              last_write_stack_id == active_stack &&
              last_write_addr == (stack_pointers[active_stack] - 1)) begin
            data_o = last_write_data;
          end else begin
            data_o = stack_mem[active_stack][stack_pointers[active_stack] - 1];
          end
        end
      end else if (state == STACK_INDEXING) begin
        if (stack_offset_i < stack_pointers[active_stack] &&
            stack_pointers[active_stack] != STACK_EMPTY) begin
          abs_index = stack_pointers[active_stack] - 1 - stack_offset_i;
          data_o = stack_mem[active_stack][abs_index];
        end
      end
    end
  end

  // --- Combinational write index for STACK_INDEXING ---
  // Computed here so the sequential block can use it with <=.
  logic [STACK_PTR_BITS-1:0] comb_write_index;
  always_comb begin
    if (stack_pointers[active_stack] != STACK_EMPTY &&
        stack_offset_i < stack_pointers[active_stack])
      comb_write_index = stack_pointers[active_stack] - 1 - stack_offset_i;
    else
      comb_write_index = 6'b0;
  end

  // --- Sequential logic ---
  integer i;

  always_ff @(posedge clk_i) begin
    if (rst_i) begin
      for (i = 0; i < 8; i = i + 1)
        stack_pointers[i] <= STACK_EMPTY;
      overflow_status <= 8'b0;
      underflow_status <= 8'b0;
      state <= STACK_IDLE;
      stack_ready_o <= 1'b1;
      stack_overflow_o <= 1'b0;
      stack_underflow_o <= 1'b0;
      last_write_valid <= 1'b0;
      pending_write <= 1'b0;
      active_stack <= 3'b0;
    end else begin
      // Clear error flags each cycle.
      stack_overflow_o <= 1'b0;
      stack_underflow_o <= 1'b0;

      case (state)
        STACK_IDLE: begin
          stack_ready_o <= 1'b1;

          if (stack_push_i) begin
            active_stack <= stack_select_i;
            state <= STACK_PUSHING;
          end else if (stack_pop_i) begin
            active_stack <= stack_select_i;
            state <= STACK_POPPING;
          end else if (stack_index_read_i || stack_index_write_i) begin
            active_stack <= stack_select_i;
            pending_write <= stack_index_write_i;
            state <= STACK_INDEXING;
          end
        end

        STACK_PUSHING: begin
          if (stack_pointers[active_stack] >= 6'd63) begin
            overflow_status[active_stack] <= 1'b1;
            stack_overflow_o <= 1'b1;
          end else begin
            stack_mem[active_stack][stack_pointers[active_stack]] <= data_i;
            stack_pointers[active_stack] <= stack_pointers[active_stack] + 1;

            last_write_stack_id <= active_stack;
            last_write_addr <= stack_pointers[active_stack];
            last_write_data <= data_i;
            last_write_valid <= 1'b1;
          end
          state <= STACK_IDLE;
          stack_ready_o <= 1'b0;
        end

        STACK_POPPING: begin
          if (stack_pointers[active_stack] == STACK_EMPTY) begin
            underflow_status[active_stack] <= 1'b1;
            stack_underflow_o <= 1'b1;
          end else begin
            stack_pointers[active_stack] <= stack_pointers[active_stack] - 1;
          end
          state <= STACK_IDLE;
          stack_ready_o <= 1'b0;
        end

        STACK_INDEXING: begin
          if (stack_offset_i >= stack_pointers[active_stack] ||
              stack_pointers[active_stack] == STACK_EMPTY) begin
            underflow_status[active_stack] <= 1'b1;
            stack_underflow_o <= 1'b1;
          end else if (pending_write) begin
            stack_mem[active_stack][comb_write_index] <= data_i;

            last_write_stack_id <= active_stack;
            last_write_addr <= comb_write_index;
            last_write_data <= data_i;
            last_write_valid <= 1'b1;
          end
          state <= STACK_IDLE;
          pending_write <= 1'b0;
          stack_ready_o <= 1'b0;
        end

        default: state <= STACK_IDLE;
      endcase
    end
  end

endmodule : stack_unit
