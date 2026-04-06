// Common bus abstraction used for both instruction fetch and data accesses.
//
// Protocol (simple valid/ready handshake, word-addressed):
//   1. Master asserts valid with addr set. For writes, wstrb != 0 and
//      write_data is driven. For reads, wstrb == 0.
//   2. Slave processes the request and asserts ready for exactly one cycle,
//      placing read results on read_data at that time.
//   3. Master may deassert valid after seeing ready.
//
// All addresses are word-aligned (each increment = one 32-bit word).
interface bus_if;
  logic [3:0]  wstrb;       // Per-byte write strobes (0 = read, nonzero = write)
  logic [31:0] write_data;  // Data to write (valid when wstrb != 0)
  logic [31:0] addr;        // Word address
  logic        valid;       // Master request is active
  logic        instr;       // High for instruction fetches (allows slave-side routing)

  logic        ready;       // Slave signals completion (one cycle pulse)
  logic [31:0] read_data;   // Data returned on reads (valid when ready is high)

  modport master(input read_data, ready, output wstrb, write_data, addr, valid, instr);

  modport slave(input wstrb, write_data, addr, valid, instr, output read_data, ready);
endinterface : bus_if
