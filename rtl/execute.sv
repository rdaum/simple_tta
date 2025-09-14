`define NUM_REGISTERS 32
`define NUM_ALUS 8

module execute (
    input wire clk_i,
    input wire rst_i,
    input wire sel_i,
    input wire [31:0] pc_i,
    input Unit src_unit_i,
    input logic [11:0] src_immediate_i,
    input logic [31:0] src_operand_i,
    input Unit dst_unit_i,
    input logic [11:0] dst_immediate_i,
    input logic [31:0] dst_operand_i,
    bus_if.master data_bus,
    output logic done_o
);
  // Registers.
  logic reg_unit_select[`NUM_REGISTERS-1:0];
  logic reg_unit_write[`NUM_REGISTERS-1:0];
  logic [31:0] reg_in_data[`NUM_REGISTERS-1:0];
  logic [31:0] reg_out_data[`NUM_REGISTERS-1:0];
  register_unit register_units[`NUM_REGISTERS-1:0] (
      .rst_i  (rst_i),
      .clk_i  (clk_i),
      .sel_i  (reg_unit_select),
      .wstrb_i(reg_unit_write),
      .data_i (reg_in_data),
      .data_o (reg_out_data)
  );

  // ALUs.
  logic alu_select[`NUM_ALUS-1:0];
  logic [31:0] alu_in_data_a[`NUM_ALUS-1:0];
  logic [31:0] alu_in_data_b[`NUM_ALUS-1:0];
  logic [31:0] alu_out_data[`NUM_ALUS-1:0];
  ALU_OPERATOR alu_operation[`NUM_ALUS-1:0];
  alu_unit alu_unit[`NUM_ALUS-1:0] (
      .rst_i(rst_i),
      .clk_i(clk_i),
      .sel_i(alu_select),
      .oper_i(alu_operation),
      .a_data_i(alu_in_data_a),
      .b_data_i(alu_in_data_b),
      .data_o(alu_out_data)
  );

  // Stack unit
  logic [2:0] stack_select;
  logic stack_push, stack_pop;
  logic [5:0] stack_offset;
  logic stack_index_read, stack_index_write;
  logic [31:0] stack_data_in, stack_data_out;
  logic stack_ready, stack_overflow, stack_underflow;

  stack_unit stack_unit_inst (
      .clk_i(clk_i),
      .rst_i(rst_i),
      .stack_select_i(stack_select),
      .stack_push_i(stack_push),
      .stack_pop_i(stack_pop),
      .stack_offset_i(stack_offset),
      .stack_index_read_i(stack_index_read),
      .stack_index_write_i(stack_index_write),
      .data_i(stack_data_in),
      .data_o(stack_data_out),
      .stack_ready_o(stack_ready),
      .stack_overflow_o(stack_overflow),
      .stack_underflow_o(stack_underflow)
  );

  // Execution state machine.
  typedef enum {
    EXEC_START_SRC,
    EXEC_SRC_MEM_RETRIEVE,
    EXEC_SRC_ALU_RETRIEVE,
    EXEC_SRC_STACK_WAIT,
    EXEC_START_DST,
    EXEC_DST_STACK_WAIT
  } ExecState;
  ExecState exec_state;
  logic [31:0] src_value;

  always @(posedge clk_i) begin
    if (rst_i) begin
      reg_unit_select = '{default: 1'b0};
      reg_unit_write = '{default: 1'b0};

      alu_select = '{default: 1'b0};
      alu_operation = '{default: ALU_NOP};

      // Initialize stack signals
      stack_push = 1'b0;
      stack_pop = 1'b0;
      stack_index_read = 1'b0;
      stack_index_write = 1'b0;

      // Initialize execution state
      exec_state = EXEC_START_SRC;

      done_o = 1'b0;
    end else if (sel_i) begin
      case (exec_state)
        EXEC_START_SRC: begin
          done_o = 1'b0;
          reg_unit_select = '{default: 1'b0};
          reg_unit_write = '{default: 1'b0};
          alu_select = '{default: 1'b0};
          data_bus.valid = 1'b0;
          data_bus.wstrb = 4'b0000;
          data_bus.instr = 1'b0;

          // Clear stack signals
          stack_push = 1'b0;
          stack_pop = 1'b0;
          stack_index_read = 1'b0;
          stack_index_write = 1'b0;
          case (src_unit_i) inside
            // Start source memory retrieval
            UNIT_MEMORY_OPERAND, UNIT_MEMORY_IMMEDIATE, UNIT_REGISTER_POINTER: begin
              case (src_unit_i)
                UNIT_MEMORY_OPERAND: data_bus.addr = src_operand_i;
                UNIT_MEMORY_IMMEDIATE: data_bus.addr = {20'b0, src_immediate_i};
                UNIT_REGISTER_POINTER: begin
                  reg_unit_select[src_immediate_i[4:0]] = 1'b1;
                  data_bus.addr = reg_out_data[src_immediate_i[4:0]];
                end
                default: data_bus.addr = 32'b0;
              endcase
              data_bus.valid = 1'b1;
              exec_state = EXEC_SRC_MEM_RETRIEVE;
            end
            UNIT_REGISTER: begin
              reg_unit_select[src_immediate_i[4:0]] = 1'b1;
              src_value = reg_out_data[src_immediate_i[4:0]];
              exec_state = EXEC_START_DST;
            end
            UNIT_ALU_LEFT: begin
              src_value  = alu_in_data_a[src_immediate_i[2:0]];
              exec_state = EXEC_START_DST;
            end
            UNIT_ALU_RIGHT: begin
              src_value  = alu_in_data_b[src_immediate_i[2:0]];
              exec_state = EXEC_START_DST;
            end
            UNIT_ALU_RESULT: begin
              alu_select[src_immediate_i[2:0]] = 1'b1;
              exec_state = EXEC_SRC_ALU_RETRIEVE;
            end
            UNIT_ABS_IMMEDIATE: begin
              src_value  = {20'b0, src_immediate_i};
              exec_state = EXEC_START_DST;
            end
            UNIT_ABS_OPERAND: begin
              src_value  = src_operand_i;
              exec_state = EXEC_START_DST;
            end
            UNIT_PC: begin
              src_value  = pc_i;
              exec_state = EXEC_START_DST;
            end
            UNIT_STACK_PUSH_POP: begin
              // Stack pop operation
              stack_select = src_immediate_i[2:0];  // Stack ID from bits 2:0
              stack_pop = 1'b1;
              exec_state = EXEC_SRC_STACK_WAIT;
            end
            UNIT_STACK_INDEX: begin
              // Stack indexed read
              stack_select = src_immediate_i[2:0];     // Stack ID from bits 2:0
              stack_offset = src_immediate_i[8:3];     // Offset from bits 8:3 (6 bits)
              stack_index_read = 1'b1;
              exec_state = EXEC_SRC_STACK_WAIT;
            end
            UNIT_NONE: begin
              src_value = 32'b0;
              // Don't waste an extra clock cycle on no-op instructions.
              if (dst_unit_i != UNIT_NONE) exec_state = EXEC_START_DST;
            end
            default: begin
              src_value  = 32'b0;
              exec_state = EXEC_START_DST;
            end
          endcase

        end
        EXEC_SRC_MEM_RETRIEVE: begin
          if (data_bus.ready) begin
            src_value = data_bus.read_data;
            data_bus.valid = 1'b0;
            exec_state = EXEC_START_DST;
          end
        end
        EXEC_SRC_ALU_RETRIEVE: begin
          src_value  = alu_out_data[src_immediate_i[2:0]];
          exec_state = EXEC_START_DST;
        end
        EXEC_SRC_STACK_WAIT: begin
          // Clear stack control signals after first cycle
          stack_push = 1'b0;
          stack_pop = 1'b0;
          stack_index_read = 1'b0;
          stack_index_write = 1'b0;

          // Wait for stack operation to complete (single cycle for register-based stacks)
          if (stack_ready) begin
            src_value  = stack_data_out;
            exec_state = EXEC_START_DST;
          end
        end
        // TODO: In some cases we might not need to wait on another cycle before performing
        // the destination. Look to optimize for that by moving the state machine along
        // without waiting for the next clock in those cases.
        // Register to register for example should be one cycle.
        EXEC_START_DST: begin
          case (dst_unit_i) inside
            UNIT_REGISTER: begin
              reg_unit_select[dst_immediate_i[4:0]] = 1'b1;
              reg_unit_write[dst_immediate_i[4:0]] = 1'b1;
              reg_in_data[dst_immediate_i[4:0]] = src_value;
              begin
                done_o = 1'b1;
                exec_state = EXEC_START_SRC;
              end
            end
            UNIT_ALU_LEFT: begin
              alu_in_data_a[dst_immediate_i[2:0]] = src_value;
              begin
                done_o = 1'b1;
                exec_state = EXEC_START_SRC;
              end
            end
            UNIT_ALU_RIGHT: begin
              alu_in_data_b[dst_immediate_i[2:0]] = src_value;
              begin
                done_o = 1'b1;
                exec_state = EXEC_START_SRC;
              end
            end
            UNIT_ALU_OPERATOR: begin
              alu_operation[dst_immediate_i[2:0]] = ALU_OPERATOR'(src_value);
              begin
                done_o = 1'b1;
                exec_state = EXEC_START_SRC;
              end
            end
            UNIT_MEMORY_OPERAND, UNIT_MEMORY_IMMEDIATE: begin
              case (dst_unit_i)
                UNIT_MEMORY_OPERAND: data_bus.addr = dst_operand_i;
                UNIT_MEMORY_IMMEDIATE: data_bus.addr = {20'b0, dst_immediate_i};
                UNIT_REGISTER_POINTER: begin
                  reg_unit_select[src_immediate_i[4:0]] = 1'b1;
                  data_bus.addr = reg_out_data[src_immediate_i[4:0]];
                end
                default: data_bus.addr = 32'b0;
              endcase


              data_bus.valid = 1'b1;
              data_bus.write_data = src_value;
              data_bus.wstrb = 4'b1111;  // TODO... hm..
              begin
                done_o = 1'b1;
                exec_state = EXEC_START_SRC;
              end
            end
            UNIT_STACK_PUSH_POP: begin
              // Stack push operation
              stack_select = dst_immediate_i[2:0];  // Stack ID from bits 2:0
              stack_data_in = src_value;
              stack_push = 1'b1;
              exec_state = EXEC_DST_STACK_WAIT;
            end
            UNIT_STACK_INDEX: begin
              // Stack indexed write
              stack_select = dst_immediate_i[2:0];     // Stack ID from bits 2:0
              stack_offset = dst_immediate_i[8:3];     // Offset from bits 8:3 (6 bits)
              stack_data_in = src_value;
              stack_index_write = 1'b1;
              exec_state = EXEC_DST_STACK_WAIT;
            end
            default: begin
              done_o = 1'b1;
              exec_state = EXEC_START_SRC;
            end

          endcase

        end
        EXEC_DST_STACK_WAIT: begin
          // Clear stack control signals after first cycle
          stack_push = 1'b0;
          stack_pop = 1'b0;
          stack_index_read = 1'b0;
          stack_index_write = 1'b0;

          // Wait for stack operation to complete (single cycle for register-based stacks)
          if (stack_ready) begin
            done_o = 1'b1;
            exec_state = EXEC_START_SRC;
          end
        end
      endcase
    end
  end
endmodule : execute
