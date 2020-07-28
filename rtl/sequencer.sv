module sequencer(
    input wire clk_i,
    input wire rst_i,
    bus_if.master instr_bus,
    output logic [31:0] pc_o,
    output logic [31:0] op_o,
    output logic [31:0] src_operand_o,
    output logic [31:0] dst_operand_o,

    input logic need_src_operand_i,
    input logic need_dst_operand_i,
    input logic sel_i,
    output wire decoder_enable_o,

    output logic done_o
);
    enum {
        SEQ_START,
        SEQ_READ_OPCODE,
        SEQ_DECODE,
        SEQ_EXEC_SOURCE,
        SEQ_EXEC_DEST,
        SEQ_READ_SRC_OPERAND_START,
        SEQ_READ_SRC_OPERAND,
        SEQ_READ_DST_OPERAND_START,
        SEQ_READ_DST_OPERAND
    } sequencer_state;

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
                    if (need_src_operand_i || need_dst_operand_i) begin
                        instr_bus.valid = 1'b1;
                        instr_bus.instr = 1'b0;
                        instr_bus.addr = pc_o + 1;
                        if (need_src_operand_i) sequencer_state = SEQ_READ_SRC_OPERAND;
                        else sequencer_state = SEQ_READ_DST_OPERAND;
                    end else begin
                        done_o = 1'b1;
                        pc_o = pc_o + 1;
                        sequencer_state = SEQ_START;
                    end
                end
                SEQ_READ_SRC_OPERAND: begin
                    if (instr_bus.ready) begin
                        src_operand_o = instr_bus.read_data;
                        pc_o = pc_o + 1;
                        if (need_dst_operand_i) sequencer_state = SEQ_READ_DST_OPERAND_START;
                        else begin
                            done_o = 1'b1;
                            pc_o = pc_o + 1;
                            sequencer_state = SEQ_START;
                        end
                    end
                end
                SEQ_READ_DST_OPERAND_START: begin
                    instr_bus.addr = pc_o + 1;
                    instr_bus.valid = 1'b1;
                    instr_bus.instr = 1'b0;
                    sequencer_state = SEQ_READ_DST_OPERAND;
                end
                SEQ_READ_DST_OPERAND: begin
                    if (instr_bus.ready) begin
                        dst_operand_o = instr_bus.read_data;
                        pc_o = pc_o + 2;
                        done_o = 1'b1;
                        sequencer_state = SEQ_START;
                    end
                end
            endcase
        end
    end

endmodule : sequencer