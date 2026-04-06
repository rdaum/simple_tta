`include "common.vh"

// Shared multi-cycle multiply/divide unit. One instance is shared across
// all 8 ALU lanes -- only one result can be read per instruction anyway.
//
// MUL: iterative shift-and-add, 32 cycles.
// DIV: non-restoring long division, 32 cycles. Divide-by-zero returns 0.
// MOD: same algorithm as DIV but returns remainder.
//
// Interface: pulse start_i for one cycle with operands latched.
// done_o pulses for one cycle when result_o is valid.
module muldiv_unit (
    input  wire        clk_i,
    input  wire        rst_i,
    input  wire        start_i,
    input  wire [3:0]  oper_i,
    input  wire [31:0] a_i,
    input  wire [31:0] b_i,
    output reg  [31:0] result_o,
    output reg         done_o
);

  localparam MULDIV_IDLE    = 1'b0;
  localparam MULDIV_RUNNING = 1'b1;
  reg state;

  reg [3:0] operation;
  reg [5:0] count;

  // MUL: accumulator + shifted operands
  reg [31:0] mul_acc;
  reg [31:0] multiplicand;
  reg [31:0] multiplier;

  // DIV: restoring long division using a 64-bit working register
  // {remainder, quotient} is shifted left each iteration
  /* verilator lint_off UNUSEDSIGNAL */
  reg [31:0] div_remainder;
  /* verilator lint_on UNUSEDSIGNAL */
  reg [31:0] div_quotient;
  reg [31:0] div_divisor;

  // Combinational trial subtraction for division
  reg [32:0] trial_sub;
  reg [31:0] shifted_remainder;
  always_comb begin
    shifted_remainder = {div_remainder[30:0], div_quotient[31]};
    trial_sub = {1'b0, shifted_remainder} - {1'b0, div_divisor};
  end

  always @(posedge clk_i) begin
    if (rst_i) begin
      state <= MULDIV_IDLE;
      done_o <= 1'b0;
      result_o <= 32'b0;
      count <= 6'b0;
      operation <= 4'b0;
      mul_acc <= 32'b0;
      multiplicand <= 32'b0;
      multiplier <= 32'b0;
      div_remainder <= 32'b0;
      div_quotient <= 32'b0;
      div_divisor <= 32'b0;
    end else begin
      done_o <= 1'b0;

      case (state)
        MULDIV_IDLE: begin
          if (start_i) begin
            operation <= oper_i;
            count <= 6'b0;

            if (oper_i == ALU_MUL) begin
              mul_acc <= 32'b0;
              multiplicand <= a_i;
              multiplier <= b_i;
              state <= MULDIV_RUNNING;
            end else begin
              // DIV or MOD
              if (b_i == 32'b0) begin
                result_o <= 32'b0;
                done_o <= 1'b1;
              end else begin
                div_remainder <= 32'b0;
                div_quotient <= a_i;
                div_divisor <= b_i;
                state <= MULDIV_RUNNING;
              end
            end
          end
        end

        MULDIV_RUNNING: begin
          if (operation == ALU_MUL) begin
            if (multiplier[0])
              mul_acc <= mul_acc + multiplicand;
            multiplicand <= multiplicand << 1;
            multiplier <= multiplier >> 1;
          end else begin
            // Restoring long division: shift left, trial subtract
            if (!trial_sub[32]) begin
              // remainder >= divisor: subtract and set quotient bit
              div_remainder <= trial_sub[31:0];
              div_quotient <= {div_quotient[30:0], 1'b1};
            end else begin
              // remainder < divisor: keep shifted value, quotient bit = 0
              div_remainder <= shifted_remainder;
              div_quotient <= {div_quotient[30:0], 1'b0};
            end
          end

          count <= count + 6'b1;
          if (count == 6'd31) begin
            if (operation == ALU_MUL) begin
              if (multiplier[0])
                result_o <= mul_acc + multiplicand;
              else
                result_o <= mul_acc;
            end else if (operation == ALU_DIV) begin
              // Final quotient bit from this cycle's trial subtraction
              if (!trial_sub[32])
                result_o <= {div_quotient[30:0], 1'b1};
              else
                result_o <= {div_quotient[30:0], 1'b0};
            end else begin
              // MOD: return remainder
              if (!trial_sub[32])
                result_o <= trial_sub[31:0];
              else
                result_o <= shifted_remainder;
            end
            done_o <= 1'b1;
            state <= MULDIV_IDLE;
          end
        end
      endcase
    end
  end
endmodule
