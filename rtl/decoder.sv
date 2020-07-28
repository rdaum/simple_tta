module decoder(
    input wire clk_i,
    input wire rst_i,
    input wire sel_i,
    input [31:0] op_i,
    output Unit src_unit_o,
    output logic [11:0] si_o,
    output logic need_src_operand_o,

    output Unit dst_unit_o,
    output logic [11:0] di_o,
    output logic need_dst_operand_o
);
    logic [31:0] src_value;
    always @(posedge clk_i) begin
        if (rst_i) begin
            src_unit_o = UNIT_NONE;
            dst_unit_o = UNIT_NONE;
            si_o = 12'b0;
            di_o = 12'b0;
            need_src_operand_o = 1'b0;
            need_dst_operand_o = 1'b0;
        end else if (sel_i) begin
            src_unit_o = Unit'(op_i[3:0]);
            si_o = op_i[15:4];
            dst_unit_o = Unit'(op_i[19:16]);
            di_o = op_i[31:20];

            need_src_operand_o = src_unit_o == UNIT_MEMORY_OPERAND ||
                src_unit_o == UNIT_ABS_OPERAND;
            need_dst_operand_o = dst_unit_o == UNIT_MEMORY_OPERAND ||
                dst_unit_o == UNIT_ABS_OPERAND;
        end
    end
            
endmodule : decoder