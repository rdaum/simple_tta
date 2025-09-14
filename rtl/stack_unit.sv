`include "common.vh"

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
    output reg [31:0] data_o,
    output reg stack_ready_o,

    // Error reporting
    output reg stack_overflow_o,
    output reg stack_underflow_o
);

  // Stack parameters - simplified design
  localparam STACK_DEPTH = 64;  // 256 bytes / 4 bytes per word = 64 words
  localparam STACK_PTR_BITS = 6;  // log2(64) = 6 bits for pointer
  localparam [STACK_PTR_BITS-1:0] STACK_EMPTY = 6'd0;  // Empty stack pointer = 0
  localparam [STACK_PTR_BITS:0] STACK_SIZE = 7'd64;  // Stack size (needs extra bit)

  // 8 independent stacks, each 64 words deep
  reg [31:0] stack_mem[0:7][0:STACK_DEPTH-1];

  // Stack pointers - empty ascending (SP points to next free location)
  reg [STACK_PTR_BITS-1:0] stack_pointers[0:7];

  // Error registers
  reg [7:0] overflow_status;  // One bit per stack
  reg [7:0] underflow_status;  // One bit per stack

  // Stack operation state
  typedef enum logic [1:0] {
    STACK_IDLE,
    STACK_PUSHING,
    STACK_POPPING,
    STACK_INDEXING
  } stack_state_t;

  stack_state_t state;
  reg [2:0] active_stack;

  // Write forwarding to handle read-after-write hazards
  reg [2:0] last_write_stack_id;
  reg [STACK_PTR_BITS-1:0] last_write_addr;
  reg [31:0] last_write_data;
  reg last_write_valid;

  // Variable for indexed operations
  reg [STACK_PTR_BITS-1:0] abs_index;
  reg [STACK_PTR_BITS-1:0] write_index;
  reg pending_write;

  // Initialize stacks
  integer i;
  initial begin
    for (i = 0; i < 8; i = i + 1) begin
      stack_pointers[i] = STACK_EMPTY;  // Start at 0 (empty stack)
    end
    overflow_status = 8'b0;
    underflow_status = 8'b0;
    state = STACK_IDLE;
    stack_ready_o = 1'b1;
    last_write_valid = 1'b0;
    pending_write = 1'b0;
  end


  // Combinational data output logic to avoid race conditions
  always_comb begin
    // Default values
    abs_index = 6'b0;

    if (rst_i) begin
      data_o = 32'b0;
    end else if (state == STACK_POPPING) begin
      // Handle pop operation data output immediately
      if (stack_pointers[active_stack] == STACK_EMPTY) begin
        data_o = 32'b0;  // Underflow case
      end else begin
        // Check for write forwarding (read-after-write hazard)
        if (last_write_valid && 
                    last_write_stack_id == active_stack && 
                    last_write_addr == (stack_pointers[active_stack] - 1)) begin
          // Forward the last written data
          data_o = last_write_data;
        end else begin
          // Normal read from memory
          data_o = stack_mem[active_stack][stack_pointers[active_stack]-1];
        end
      end
    end else if (state == STACK_INDEXING) begin
      // Handle indexed read operation
      // For empty ascending: offset 0 = top of stack (SP-1), offset 1 = SP-2, etc.
      if (stack_offset_i >= stack_pointers[active_stack] || stack_pointers[active_stack] == STACK_EMPTY) begin
        data_o = 32'b0;  // Out of bounds or empty stack
      end else begin
        abs_index = stack_pointers[active_stack] - 1 - stack_offset_i;
        data_o = stack_mem[active_stack][abs_index];
      end
    end else begin
      data_o = 32'b0;  // Default value when idle or pushing
    end
  end

  // Stack operations
  always_ff @(posedge clk_i) begin
    if (rst_i) begin
      for (i = 0; i < 8; i = i + 1) begin
        stack_pointers[i] <= STACK_EMPTY;
      end
      overflow_status <= 8'b0;
      underflow_status <= 8'b0;
      state <= STACK_IDLE;
      stack_ready_o <= 1'b1;
      stack_overflow_o <= 1'b0;
      stack_underflow_o <= 1'b0;

      // Reset write forwarding
      last_write_valid <= 1'b0;
      pending_write <= 1'b0;
    end else begin
      // Clear error flags by default
      stack_overflow_o  <= 1'b0;
      stack_underflow_o <= 1'b0;

      case (state)
        STACK_IDLE: begin
          stack_ready_o <= 1'b1;  // Always ready when idle

          if (stack_push_i) begin
            active_stack <= stack_select_i;
            state <= STACK_PUSHING;
            // Keep ready high for single-cycle operation
          end else if (stack_pop_i) begin
            active_stack <= stack_select_i;
            state <= STACK_POPPING;
            // Keep ready high for single-cycle operation
          end else if (stack_index_read_i || stack_index_write_i) begin
            active_stack <= stack_select_i;
            state <= STACK_INDEXING;
            // Latch write intent to avoid race condition with execute module
            pending_write <= stack_index_write_i;
            // Keep ready high for single-cycle operation
          end
        end

        STACK_PUSHING: begin
          // Check for overflow (stack full when pointer would exceed bounds)
          if (stack_pointers[active_stack] >= 6'd63) begin
            overflow_status[active_stack] <= 1'b1;
            stack_overflow_o <= 1'b1;
          end else begin
            // Push: store at SP, then increment SP (empty ascending)
            stack_mem[active_stack][stack_pointers[active_stack]] <= data_i;
            stack_pointers[active_stack] <= stack_pointers[active_stack] + 1;

            // Record write for forwarding
            last_write_stack_id <= active_stack;
            last_write_addr <= stack_pointers[active_stack];
            last_write_data <= data_i;
            last_write_valid <= 1'b1;
          end
          state <= STACK_IDLE;
          // Make stack not ready for one cycle to ensure proper sequencing
          stack_ready_o <= 1'b0;
        end

        STACK_POPPING: begin
          // Check for underflow (stack empty when pointer is at 0)
          if (stack_pointers[active_stack] == STACK_EMPTY) begin
            underflow_status[active_stack] <= 1'b1;
            stack_underflow_o <= 1'b1;
          end else begin
            // Pop: decrement SP, then read from SP (empty ascending)
            stack_pointers[active_stack] <= stack_pointers[active_stack] - 1;
          end
          state <= STACK_IDLE;
          // Make stack not ready for one cycle to ensure proper sequencing
          stack_ready_o <= 1'b0;
        end

        STACK_INDEXING: begin
          // Calculate absolute index from stack pointer - offset - 1
          // For empty ascending: offset 0 = top of stack (SP-1), offset 1 = SP-2, etc.

          if (stack_offset_i >= stack_pointers[active_stack] || stack_pointers[active_stack] == STACK_EMPTY) begin
            // Index out of bounds or empty stack
            underflow_status[active_stack] <= 1'b1;
            stack_underflow_o <= 1'b1;
          end else begin
            write_index = stack_pointers[active_stack] - 1 - stack_offset_i;
            if (pending_write) begin
              stack_mem[active_stack][write_index] <= data_i;

              // Update write forwarding for read-after-write hazards
              last_write_stack_id <= active_stack;
              last_write_addr <= write_index;
              last_write_data <= data_i;
              last_write_valid <= 1'b1;
            end
          end
          state <= STACK_IDLE;
          pending_write <= 1'b0;  // Clear the pending write flag
          // Make stack not ready for one cycle to ensure proper sequencing
          stack_ready_o <= 1'b0;
        end

        default: state <= STACK_IDLE;
      endcase
    end
  end

endmodule
