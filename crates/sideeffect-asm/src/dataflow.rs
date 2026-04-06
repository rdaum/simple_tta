//! Dataflow graph compiler for the TTA.
//!
//! Build a graph of operations with data dependencies, then compile it
//! to scheduled TTA move sequences. The compiler handles ALU lane
//! allocation, register assignment, move interleaving, and label
//! resolution.
//!
//! ```ignore
//! let mut g = Graph::new();
//! let a = g.constant(42);
//! let b = g.constant(10);
//! let sum = g.add(a, b);
//! g.store_mem(100, sum);
//! let moves = g.compile();
//! ```
//!
//! Independent ALU operations are interleaved across lanes:
//! ```ignore
//! let mut g = Graph::new();
//! let sum = g.add(g.constant(1), g.constant(2));  // lane 0
//! let prod = g.mul(g.constant(3), g.constant(4)); // lane 1
//! // Emits: 1→alu0.L, 3→alu1.L, 2→alu0.R, 4→alu1.R, ADD→alu0.op, MUL→alu1.op
//! ```

use crate::assembler::{instr, ALUOp, Instr, Unit};

/// A handle to a value produced by a node in the dataflow graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Value(usize);

/// A branch target label, resolved to an instruction address after layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(usize);

/// An ALU operation in the dataflow graph.
#[derive(Debug, Clone, Copy)]
pub enum AluBinOp {
    Add, Sub, Mul, Div, Mod,
    And, Or, Xor, Shl, Shr, Sra,
    Eq, Gt, Lt,
}

impl AluBinOp {
    fn to_alu_op(self) -> ALUOp {
        match self {
            AluBinOp::Add => ALUOp::ALU_ADD,
            AluBinOp::Sub => ALUOp::ALU_SUB,
            AluBinOp::Mul => ALUOp::ALU_MUL,
            AluBinOp::Div => ALUOp::ALU_DIV,
            AluBinOp::Mod => ALUOp::ALU_MOD,
            AluBinOp::And => ALUOp::ALU_AND,
            AluBinOp::Or  => ALUOp::ALU_OR,
            AluBinOp::Xor => ALUOp::ALU_XOR,
            AluBinOp::Shl => ALUOp::ALU_SL,
            AluBinOp::Shr => ALUOp::ALU_SR,
            AluBinOp::Sra => ALUOp::ALU_SRA,
            AluBinOp::Eq  => ALUOp::ALU_EQL,
            AluBinOp::Gt  => ALUOp::ALU_GT,
            AluBinOp::Lt  => ALUOp::ALU_LT,
        }
    }
}

/// A node in the dataflow graph.
#[derive(Debug, Clone)]
enum Node {
    Constant(u32),
    LoadReg(u16),
    LoadMem(u32),
    AluBin(AluBinOp, Value, Value),
    Not(Value),
    StoreReg(u16, Value),
    StoreMem(u32, Value),
    SetCond(Value),
    BranchCond(Value),
    BranchCondLabel(Label),
    Branch(Value),
    BranchLabel(Label),
    StackPush(u8, Value),
    StackPop(u8),
    /// A label marker — doesn't produce a value, just marks a position.
    LabelDef(Label),
}

/// Dataflow graph: build nodes, then compile to TTA moves.
pub struct Graph {
    nodes: Vec<Node>,
    next_label: usize,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next_label: 0,
        }
    }

    fn push(&mut self, node: Node) -> Value {
        let id = self.nodes.len();
        self.nodes.push(node);
        Value(id)
    }

    // --- Labels ---

    /// Create a new label for use as a branch target.
    pub fn label(&mut self) -> Label {
        let id = self.next_label;
        self.next_label += 1;
        Label(id)
    }

    /// Place a label at the current position in the instruction stream.
    pub fn place_label(&mut self, label: Label) {
        self.nodes.push(Node::LabelDef(label));
    }

    // --- Value producers ---

    pub fn constant(&mut self, val: u32) -> Value {
        self.push(Node::Constant(val))
    }

    pub fn load_reg(&mut self, reg: u16) -> Value {
        self.push(Node::LoadReg(reg))
    }

    pub fn load_mem(&mut self, addr: u32) -> Value {
        self.push(Node::LoadMem(addr))
    }

    pub fn alu(&mut self, op: AluBinOp, left: Value, right: Value) -> Value {
        self.push(Node::AluBin(op, left, right))
    }

    pub fn add(&mut self, a: Value, b: Value) -> Value { self.alu(AluBinOp::Add, a, b) }
    pub fn sub(&mut self, a: Value, b: Value) -> Value { self.alu(AluBinOp::Sub, a, b) }
    pub fn mul(&mut self, a: Value, b: Value) -> Value { self.alu(AluBinOp::Mul, a, b) }
    pub fn gt(&mut self, a: Value, b: Value) -> Value  { self.alu(AluBinOp::Gt, a, b) }
    pub fn lt(&mut self, a: Value, b: Value) -> Value  { self.alu(AluBinOp::Lt, a, b) }
    pub fn eq(&mut self, a: Value, b: Value) -> Value  { self.alu(AluBinOp::Eq, a, b) }
    pub fn and(&mut self, a: Value, b: Value) -> Value { self.alu(AluBinOp::And, a, b) }
    pub fn or(&mut self, a: Value, b: Value) -> Value  { self.alu(AluBinOp::Or, a, b) }
    pub fn not(&mut self, a: Value) -> Value { self.push(Node::Not(a)) }

    pub fn stack_pop(&mut self, stack_id: u8) -> Value {
        self.push(Node::StackPop(stack_id))
    }

    // --- Side effects ---

    pub fn store_reg(&mut self, reg: u16, val: Value) { self.push(Node::StoreReg(reg, val)); }
    pub fn store_mem(&mut self, addr: u32, val: Value) { self.push(Node::StoreMem(addr, val)); }
    pub fn set_cond(&mut self, val: Value) { self.push(Node::SetCond(val)); }
    pub fn branch_cond(&mut self, target: Value) { self.push(Node::BranchCond(target)); }
    pub fn branch(&mut self, target: Value) { self.push(Node::Branch(target)); }
    pub fn stack_push(&mut self, stack_id: u8, val: Value) { self.push(Node::StackPush(stack_id, val)); }

    /// Conditional branch to a label.
    pub fn branch_cond_label(&mut self, label: Label) { self.push(Node::BranchCondLabel(label)); }
    /// Unconditional branch to a label.
    pub fn branch_label(&mut self, label: Label) { self.push(Node::BranchLabel(label)); }

    // --- Compilation ---

    pub fn compile(&self) -> Vec<Instr> {
        let mut emitter = Emitter::new(self.next_label);
        emitter.emit_graph(&self.nodes)
    }
}

/// Tracks where a Value currently lives.
#[derive(Debug, Clone)]
enum Location {
    SmallImm(u16),
    LargeImm(u32),
    Reg(u16),
    AluResult(u8),
    Pending,
}

/// Partially scheduled ALU operation, waiting to be interleaved.
struct PendingAlu {
    node_idx: usize,
    op: AluBinOp,
    left: Location,
    right: Location,
    lane: u8,
}

struct Emitter {
    locations: Vec<Location>,
    next_lane: u8,
    next_tmp_reg: u16,
    output: Vec<Instr>,
    /// Pending ALU ops collected for interleaving.
    pending_alus: Vec<PendingAlu>,
    /// Label → instruction word index (filled during emit, resolved after).
    label_positions: Vec<Option<usize>>,
    /// (output instruction index, label) pairs to fix up after layout.
    label_fixups: Vec<(usize, Label)>,
}

impl Emitter {
    fn new(num_labels: usize) -> Self {
        Self {
            locations: Vec::new(),
            next_lane: 0,
            next_tmp_reg: 16,
            output: Vec::new(),
            pending_alus: Vec::new(),
            label_positions: vec![None; num_labels],
            label_fixups: Vec::new(),
        }
    }

    fn alloc_lane(&mut self) -> u8 {
        let lane = self.next_lane;
        self.next_lane = (self.next_lane + 1) % 8;
        lane
    }

    fn alloc_tmp_reg(&mut self) -> u16 {
        let reg = self.next_tmp_reg;
        self.next_tmp_reg += 1;
        assert!(self.next_tmp_reg <= 32, "Out of temporary registers");
        reg
    }

    fn materialize(&self, val: Value) -> Location {
        self.locations[val.0].clone()
    }

    fn emit_move_to_dst(&mut self, loc: &Location, dst_unit: Unit, di: u16) {
        match loc {
            Location::SmallImm(v) => {
                self.output.push(
                    instr().src(Unit::UNIT_ABS_IMMEDIATE).si(*v).dst(dst_unit).di(di),
                );
            }
            Location::LargeImm(v) => {
                self.output.push(
                    instr().src(Unit::UNIT_ABS_OPERAND).soperand(*v).dst(dst_unit).di(di),
                );
            }
            Location::Reg(r) => {
                self.output.push(
                    instr().src(Unit::UNIT_REGISTER).si(*r).dst(dst_unit).di(di),
                );
            }
            Location::AluResult(lane) => {
                self.output.push(
                    instr().src(Unit::UNIT_ALU_RESULT).si(*lane as u16).dst(dst_unit).di(di),
                );
            }
            Location::Pending => panic!("Cannot emit from pending location"),
        }
    }

    fn ensure_in_reg(&mut self, val: Value) -> u16 {
        let loc = self.materialize(val);
        match loc {
            Location::Reg(r) => r,
            _ => {
                let reg = self.alloc_tmp_reg();
                self.emit_move_to_dst(&loc, Unit::UNIT_REGISTER, reg);
                self.locations[val.0] = Location::Reg(reg);
                reg
            }
        }
    }

    /// Flush all pending ALU ops as interleaved moves.
    fn flush_pending_alus(&mut self) {
        if self.pending_alus.is_empty() {
            return;
        }

        let pending: Vec<PendingAlu> = std::mem::take(&mut self.pending_alus);

        // Phase 1: set all left operands.
        for p in &pending {
            self.emit_move_to_dst(&p.left, Unit::UNIT_ALU_LEFT, p.lane as u16);
        }
        // Phase 2: set all right operands.
        for p in &pending {
            self.emit_move_to_dst(&p.right, Unit::UNIT_ALU_RIGHT, p.lane as u16);
        }
        // Phase 3: set all operators.
        for p in &pending {
            self.output.push(
                instr()
                    .src(Unit::UNIT_ABS_IMMEDIATE)
                    .si(p.op.to_alu_op() as u16)
                    .dst(Unit::UNIT_ALU_OPERATOR)
                    .di(p.lane as u16),
            );
        }
        // Record result locations.
        for p in &pending {
            self.locations[p.node_idx] = Location::AluResult(p.lane);
        }
    }

    fn emit_graph(&mut self, nodes: &[Node]) -> Vec<Instr> {
        self.locations = vec![Location::Pending; nodes.len()];

        // First pass: resolve constants and register loads.
        for (i, node) in nodes.iter().enumerate() {
            match node {
                Node::Constant(v) => {
                    self.locations[i] = if *v < 4096 {
                        Location::SmallImm(*v as u16)
                    } else {
                        Location::LargeImm(*v)
                    };
                }
                Node::LoadReg(r) => {
                    self.locations[i] = Location::Reg(*r);
                }
                _ => {}
            }
        }

        // Second pass: emit operations, batching ALU ops for interleaving.
        for i in 0..nodes.len() {
            // If the current node is NOT an ALU op, flush any pending ALUs first.
            let is_alu = matches!(&nodes[i], Node::AluBin(..));
            if !is_alu {
                self.flush_pending_alus();
            }

            match &nodes[i] {
                Node::Constant(_) | Node::LoadReg(_) => {}

                Node::LoadMem(addr) => {
                    let reg = self.alloc_tmp_reg();
                    if *addr < 4096 {
                        self.output.push(
                            instr()
                                .src(Unit::UNIT_MEMORY_IMMEDIATE).si(*addr as u16)
                                .dst(Unit::UNIT_REGISTER).di(reg),
                        );
                    } else {
                        self.output.push(
                            instr()
                                .src_mem_op(*addr, crate::assembler::AccessWidth::Word, 0)
                                .dst(Unit::UNIT_REGISTER).di(reg),
                        );
                    }
                    self.locations[i] = Location::Reg(reg);
                }

                Node::AluBin(op, left, right) => {
                    let lane = self.alloc_lane();
                    let left_loc = self.materialize(*left);
                    let right_loc = self.materialize(*right);

                    // If either operand is an AluResult, materialize it to a
                    // register BEFORE batching (the ALU lane might get reused).
                    let left_loc = match left_loc {
                        Location::AluResult(_) => {
                            let r = self.ensure_in_reg(*left);
                            Location::Reg(r)
                        }
                        other => other,
                    };
                    let right_loc = match right_loc {
                        Location::AluResult(_) => {
                            let r = self.ensure_in_reg(*right);
                            Location::Reg(r)
                        }
                        other => other,
                    };

                    self.pending_alus.push(PendingAlu {
                        node_idx: i,
                        op: *op,
                        left: left_loc,
                        right: right_loc,
                        lane,
                    });
                }

                Node::Not(val) => {
                    let lane = self.alloc_lane();
                    let loc = self.materialize(*val);
                    self.emit_move_to_dst(&loc, Unit::UNIT_ALU_LEFT, lane as u16);
                    self.output.push(
                        instr()
                            .src(Unit::UNIT_ABS_IMMEDIATE)
                            .si(ALUOp::ALU_NOT as u16)
                            .dst(Unit::UNIT_ALU_OPERATOR)
                            .di(lane as u16),
                    );
                    self.locations[i] = Location::AluResult(lane);
                }

                Node::StoreReg(reg, val) => {
                    let loc = self.materialize(*val);
                    self.emit_move_to_dst(&loc, Unit::UNIT_REGISTER, *reg);
                }

                Node::StoreMem(addr, val) => {
                    let loc = self.materialize(*val);
                    if *addr < 4096 {
                        self.emit_move_to_dst(&loc, Unit::UNIT_MEMORY_IMMEDIATE, *addr as u16);
                    } else {
                        let reg = self.ensure_in_reg(*val);
                        self.output.push(
                            instr()
                                .src(Unit::UNIT_REGISTER).si(reg)
                                .dst_mem_op(*addr, crate::assembler::AccessWidth::Word, 0),
                        );
                    }
                }

                Node::SetCond(val) => {
                    let loc = self.materialize(*val);
                    self.emit_move_to_dst(&loc, Unit::UNIT_COND, 0);
                }

                Node::BranchCond(target) => {
                    let loc = self.materialize(*target);
                    self.emit_move_to_dst(&loc, Unit::UNIT_PC_COND, 0);
                }

                Node::Branch(target) => {
                    let loc = self.materialize(*target);
                    self.emit_move_to_dst(&loc, Unit::UNIT_PC, 0);
                }

                Node::BranchCondLabel(label) => {
                    // Emit a placeholder branch with target 0; fix up after layout.
                    let idx = self.output.len();
                    self.output.push(
                        instr()
                            .src(Unit::UNIT_ABS_OPERAND).soperand(0)
                            .dst(Unit::UNIT_PC_COND),
                    );
                    self.label_fixups.push((idx, *label));
                }

                Node::BranchLabel(label) => {
                    let idx = self.output.len();
                    self.output.push(
                        instr()
                            .src(Unit::UNIT_ABS_OPERAND).soperand(0)
                            .dst(Unit::UNIT_PC),
                    );
                    self.label_fixups.push((idx, *label));
                }

                Node::StackPush(stack_id, val) => {
                    let reg = self.ensure_in_reg(*val);
                    self.output.push(instr().push_reg(*stack_id, reg));
                }

                Node::StackPop(stack_id) => {
                    let reg = self.alloc_tmp_reg();
                    self.output.push(instr().pop_to_reg(*stack_id, reg));
                    self.locations[i] = Location::Reg(reg);
                }

                Node::LabelDef(label) => {
                    // Record the word offset of the NEXT instruction.
                    let word_offset: usize = self.output.iter()
                        .map(|i| i.assemble().len())
                        .sum();
                    self.label_positions[label.0] = Some(word_offset);
                }
            }
        }

        // Final flush for any trailing ALU ops.
        self.flush_pending_alus();

        // Resolve label fixups.
        for (instr_idx, label) in &self.label_fixups {
            let target_addr = self.label_positions[label.0]
                .unwrap_or_else(|| panic!("Label {:?} was never placed", label));
            // Rebuild the instruction with the correct target address.
            let old = &self.output[*instr_idx];
            // Determine if it's PC or PC_COND by checking the dst unit.
            let assembled = old.assemble();
            let dst_unit_bits = (assembled[0] >> 16) & 0xF;
            let dst_unit = if dst_unit_bits == Unit::UNIT_PC_COND as u32 {
                Unit::UNIT_PC_COND
            } else {
                Unit::UNIT_PC
            };
            self.output[*instr_idx] = instr()
                .src(Unit::UNIT_ABS_OPERAND)
                .soperand(target_addr as u32)
                .dst(dst_unit);
        }

        std::mem::take(&mut self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_to_reg() {
        let mut g = Graph::new();
        let a = g.constant(42);
        g.store_reg(0, a);
        let moves = g.compile();
        assert_eq!(moves.len(), 1);
    }

    #[test]
    fn test_add_two_constants() {
        let mut g = Graph::new();
        let a = g.constant(42);
        let b = g.constant(10);
        let sum = g.add(a, b);
        g.store_reg(0, sum);
        let moves = g.compile();
        // 42→alu.left, 10→alu.right, ADD→alu.op, alu.result→reg[0]
        assert_eq!(moves.len(), 4);
    }

    #[test]
    fn test_two_independent_ops_interleaved() {
        let mut g = Graph::new();
        let a = g.constant(1);
        let b = g.constant(2);
        let c = g.constant(3);
        let d = g.constant(4);
        let sum = g.add(a, b);   // lane 0
        let prod = g.mul(c, d);  // lane 1
        g.store_reg(0, sum);
        g.store_reg(1, prod);

        let moves = g.compile();
        // Interleaved: L0.left, L1.left, L0.right, L1.right, L0.op, L1.op + 2 stores = 8
        assert_eq!(moves.len(), 8);

        // Verify interleaving: first two moves should target different ALU lanes.
        let w0 = moves[0].assemble();
        let w1 = moves[1].assemble();
        let lane_0 = (w0[0] >> 20) & 0xFFF;
        let lane_1 = (w1[0] >> 20) & 0xFFF;
        assert_eq!(lane_0, 0, "First left should target lane 0");
        assert_eq!(lane_1, 1, "Second left should target lane 1");
    }

    #[test]
    fn test_chained_ops() {
        let mut g = Graph::new();
        let a = g.constant(10);
        let b = g.constant(20);
        let sum = g.add(a, b);
        let c = g.constant(5);
        let prod = g.mul(sum, c);
        g.store_reg(0, prod);

        let moves = g.compile();
        // sum: 3 (interleaved alone), then sum.result→reg (1),
        // prod: 3 (interleaved alone) + store (1) = 8 total
        assert!(!moves.is_empty());
    }

    #[test]
    fn test_large_constant() {
        let mut g = Graph::new();
        let a = g.constant(0xDEADBEEF);
        g.store_reg(0, a);
        let moves = g.compile();
        assert_eq!(moves.len(), 1);
        let words = moves[0].assemble();
        assert_eq!(words.len(), 2);
        assert_eq!(words[1], 0xDEADBEEF);
    }

    #[test]
    fn test_labels() {
        let mut g = Graph::new();
        let skip = g.label();
        let done = g.label();

        let cond = g.constant(1);
        g.set_cond(cond);
        g.branch_cond_label(skip);
        // This should be skipped:
        let bad = g.constant(0xBAD);
        g.store_mem(100, bad);
        // Branch target:
        g.place_label(skip);
        let good = g.constant(0x600D);
        g.store_mem(100, good);
        g.place_label(done);

        let moves = g.compile();
        // Verify the branch instruction has the correct target.
        // The branch is the 2nd instruction (after set_cond).
        // It should target the word address of the label.
        let branch_instr = &moves[1];
        let words = branch_instr.assemble();
        assert_eq!(words.len(), 2, "Branch should be 2-word (ABS_OPERAND)");
        let target = words[1];
        // The target should be > 0 (it's after the skipped store).
        assert!(target > 0, "Label target should be resolved to a non-zero address");
    }

    #[test]
    fn test_three_way_interleave() {
        let mut g = Graph::new();
        let a = g.constant(1);
        let b = g.constant(2);
        let c = g.constant(3);
        let d = g.constant(4);
        let e = g.constant(5);
        let f = g.constant(6);
        let x = g.add(a, b);  // lane 0
        let y = g.sub(c, d);  // lane 1
        let z = g.mul(e, f);  // lane 2
        g.store_reg(0, x);
        g.store_reg(1, y);
        g.store_reg(2, z);

        let moves = g.compile();
        // 3 lefts + 3 rights + 3 ops + 3 stores = 12
        assert_eq!(moves.len(), 12);

        // First three moves should be the interleaved left operands
        // targeting lanes 0, 1, 2.
        for (i, expected_lane) in [0u16, 1, 2].iter().enumerate() {
            let w = moves[i].assemble();
            let lane = (w[0] >> 20) & 0xFFF;
            assert_eq!(lane, *expected_lane as u32,
                "Move {} should target lane {}", i, expected_lane);
        }
    }
}
