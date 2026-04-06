`include "common.vh"

// Write barrier FIFO for hardware-assisted garbage collection.
//
// A small circular buffer that logs memory addresses written by
// pointer stores. The mutator pushes addresses; the GC pops them
// to find dirty regions.
//
// 32 entries deep. When full, further pushes are silently dropped
// and barrier_overflow_o pulses (the GC should drain before this
// happens). When empty, pops return 0 and barrier_empty_o is high.
module barrier_unit (
    input wire clk_i,
    input wire rst_i,

    // Push interface (mutator writes)
    input wire        push_i,
    input wire [31:0] data_i,

    // Pop interface (GC reads)
    input wire        pop_i,
    output reg [31:0] data_o,
    output wire       ready_o,

    // Status
    output wire       barrier_empty_o,
    output wire       barrier_full_o,
    /* verilator lint_off UNUSEDSIGNAL */
    output reg        barrier_overflow_o
    /* verilator lint_on UNUSEDSIGNAL */
);

  localparam DEPTH = 32;
  localparam PTR_BITS = 5;

  reg [31:0] fifo [0:DEPTH-1];
  reg [PTR_BITS:0] count;  // 6 bits to hold 0..32
  reg [PTR_BITS-1:0] wr_ptr;
  reg [PTR_BITS-1:0] rd_ptr;

  assign barrier_empty_o = (count == 0);
  assign barrier_full_o  = (count == DEPTH);

  // State machine for multi-cycle handshake with execute.
  typedef enum logic [1:0] {
    BARRIER_IDLE,
    BARRIER_PUSHING,
    BARRIER_POPPING
  } barrier_state_t;

  barrier_state_t state;
  assign ready_o = (state == BARRIER_IDLE);

  // Pop output: latched on the POPPING cycle so it remains stable
  // when the barrier returns to IDLE and execute reads it.
  // (rd_ptr advances via <= in POPPING, so the combinational read
  // during POPPING sees the correct entry before the pointer moves.)

  always_ff @(posedge clk_i) begin
    if (rst_i) begin
      count <= 0;
      wr_ptr <= 0;
      rd_ptr <= 0;
      data_o <= 32'b0;
      state <= BARRIER_IDLE;
      barrier_overflow_o <= 1'b0;
    end else begin
      barrier_overflow_o <= 1'b0;

      case (state)
        BARRIER_IDLE: begin
          if (push_i) begin
            state <= BARRIER_PUSHING;
          end else if (pop_i) begin
            state <= BARRIER_POPPING;
          end
        end

        BARRIER_PUSHING: begin
          if (count < DEPTH) begin
            fifo[wr_ptr] <= data_i;
            wr_ptr <= wr_ptr + 1;
            count <= count + 1;
          end else begin
            barrier_overflow_o <= 1'b1;
          end
          state <= BARRIER_IDLE;
        end

        BARRIER_POPPING: begin
          if (count != 0) begin
            data_o <= fifo[rd_ptr];
            rd_ptr <= rd_ptr + 1;
            count <= count - 1;
          end else begin
            data_o <= 32'b0;
          end
          state <= BARRIER_IDLE;
        end

        default: state <= BARRIER_IDLE;
      endcase
    end
  end

endmodule
