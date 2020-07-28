interface bus_if;
    logic [3:0] wstrb;
    logic [31:0] write_data;
    logic [31:0] addr;
    logic valid;
    logic instr;

    logic ready;
    logic [31:0] read_data;

    modport master (
      input read_data, ready,
      output wstrb, write_data, addr, valid, instr
    );

    modport slave (
        input wstrb, write_data, addr, valid, instr,
        output  read_data, ready
    );
endinterface : bus_if
