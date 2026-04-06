`include "common.vh"

// Shared multi-cycle multiply/divide unit. One instance is shared across
// all ALU lanes — only one result can be read per instruction anyway.
//
// Arithmetic operates on the VAL_WIDTH-bit value portion only.
// The tag from the A (left) operand is preserved in the result.
//
// MUL: iterative shift-and-add, 32 cycles.
// DIV: non-restoring long division, 32 cycles. Divide-by-zero returns 0.
// MOD: same algorithm as DIV but returns remainder.
module muldiv_unit (
    input  wire        clk_i,
    input  wire        rst_i,
    input  wire        start_i,
    input  wire [3:0]  oper_i,
    input  wire [DATA_WIDTH-1:0] a_i,
    /* verilator lint_off UNUSEDSIGNAL */
    input  wire [DATA_WIDTH-1:0] b_i,
    /* verilator lint_on UNUSEDSIGNAL */
    output reg  [DATA_WIDTH-1:0] result_o,
    output reg         done_o
);

  localparam MULDIV_IDLE    = 1'b0;
  localparam MULDIV_RUNNING = 1'b1;
  reg state;

  reg [3:0] operation;
  reg [5:0] count;
  reg [TAG_WIDTH-1:0] saved_tag;

  // MUL: accumulator + shifted operands (value portion only)
  reg [VAL_WIDTH-1:0] mul_acc;
  reg [VAL_WIDTH-1:0] multiplicand;
  reg [VAL_WIDTH-1:0] multiplier;

  // DIV: restoring long division (value portion only)
  /* verilator lint_off UNUSEDSIGNAL */
  reg [VAL_WIDTH-1:0] div_remainder;
  /* verilator lint_on UNUSEDSIGNAL */
  reg [VAL_WIDTH-1:0] div_quotient;
  reg [VAL_WIDTH-1:0] div_divisor;

  // Combinational trial subtraction for division
  reg [VAL_WIDTH:0] trial_sub;
  reg [VAL_WIDTH-1:0] shifted_remainder;
  always_comb begin
    shifted_remainder = {div_remainder[VAL_WIDTH-2:0], div_quotient[VAL_WIDTH-1]};
    trial_sub = {1'b0, shifted_remainder} - {1'b0, div_divisor};
  end

  always @(posedge clk_i) begin
    if (rst_i) begin
      state <= MULDIV_IDLE;
      done_o <= 1'b0;
      result_o <= {DATA_WIDTH{1'b0}};
      count <= 6'b0;
      operation <= 4'b0;
      saved_tag <= {TAG_WIDTH{1'b0}};
      mul_acc <= {VAL_WIDTH{1'b0}};
      multiplicand <= {VAL_WIDTH{1'b0}};
      multiplier <= {VAL_WIDTH{1'b0}};
      div_remainder <= {VAL_WIDTH{1'b0}};
      div_quotient <= {VAL_WIDTH{1'b0}};
      div_divisor <= {VAL_WIDTH{1'b0}};
    end else begin
      done_o <= 1'b0;

      case (state)
        MULDIV_IDLE: begin
          if (start_i) begin
            operation <= oper_i;
            count <= 6'b0;
            saved_tag <= a_i[DATA_WIDTH-1:VAL_WIDTH];

            if (oper_i == ALU_MUL) begin
              mul_acc <= {VAL_WIDTH{1'b0}};
              multiplicand <= a_i[VAL_WIDTH-1:0];
              multiplier <= b_i[VAL_WIDTH-1:0];
              state <= MULDIV_RUNNING;
            end else begin
              if (b_i[VAL_WIDTH-1:0] == {VAL_WIDTH{1'b0}}) begin
                result_o <= {a_i[DATA_WIDTH-1:VAL_WIDTH], {VAL_WIDTH{1'b0}}};
                done_o <= 1'b1;
              end else begin
                div_remainder <= {VAL_WIDTH{1'b0}};
                div_quotient <= a_i[VAL_WIDTH-1:0];
                div_divisor <= b_i[VAL_WIDTH-1:0];
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
            if (!trial_sub[VAL_WIDTH]) begin
              div_remainder <= trial_sub[VAL_WIDTH-1:0];
              div_quotient <= {div_quotient[VAL_WIDTH-2:0], 1'b1};
            end else begin
              div_remainder <= shifted_remainder;
              div_quotient <= {div_quotient[VAL_WIDTH-2:0], 1'b0};
            end
          end

          count <= count + 6'b1;
          if (count == 6'd31) begin
            if (operation == ALU_MUL) begin
              if (multiplier[0])
                result_o <= {saved_tag, mul_acc + multiplicand};
              else
                result_o <= {saved_tag, mul_acc};
            end else if (operation == ALU_DIV) begin
              if (!trial_sub[VAL_WIDTH])
                result_o <= {saved_tag, div_quotient[VAL_WIDTH-2:0], 1'b1};
              else
                result_o <= {saved_tag, div_quotient[VAL_WIDTH-2:0], 1'b0};
            end else begin
              if (!trial_sub[VAL_WIDTH])
                result_o <= {saved_tag, trial_sub[VAL_WIDTH-1:0]};
              else
                result_o <= {saved_tag, shifted_remainder};
            end
            done_o <= 1'b1;
            state <= MULDIV_IDLE;
          end
        end
      endcase
    end
  end
endmodule
