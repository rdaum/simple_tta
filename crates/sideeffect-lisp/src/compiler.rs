use sideeffect_asm::assembler::{instr, Instr, Unit};
use sideeffect_asm::ALUOp;

use crate::types::regs::*;
use crate::types::stacks::*;
use crate::types::{Expr, Tag};

/// Compile-time environment: tracks lexical variable bindings.
/// Each scope is a list of variable names. Index 0 is the innermost scope.
#[derive(Debug, Clone)]
struct Env {
    scopes: Vec<Vec<String>>,
}

impl Env {
    fn new() -> Self {
        Self { scopes: Vec::new() }
    }

    fn push_scope(&mut self, vars: Vec<String>) {
        self.scopes.push(vars);
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Look up a variable. Returns (depth, slot) where depth is
    /// the number of parent-chain hops and slot is the index within
    /// that frame (offset by 1 in the runtime frame, since slot 0
    /// is the parent pointer).
    fn lookup(&self, name: &str) -> Option<(usize, usize)> {
        for (depth, scope) in self.scopes.iter().rev().enumerate() {
            if let Some(slot) = scope.iter().position(|s| s == name) {
                return Some((depth, slot));
            }
        }
        None
    }
}

/// Top-level definitions: maps global names to instruction addresses.
#[derive(Debug, Clone)]
struct GlobalDef {
    name: String,
    /// Word address in instruction memory where the function body starts.
    addr: u32,
    /// Number of parameters.
    #[allow(dead_code)]
    arity: usize,
}

/// Compilation output: instruction words + metadata.
#[derive(Debug)]
pub struct Program {
    /// Assembled instruction words (flat, concatenated).
    pub code: Vec<u32>,
    /// Address of the entry point (main/top-level code).
    pub entry: u32,
}

/// Compiler state.
struct Compiler {
    /// Accumulated instruction words.
    code: Vec<u32>,
    /// Compile-time lexical environment.
    env: Env,
    /// Global function definitions.
    globals: Vec<GlobalDef>,
    /// Pending forward references (reserved for future use).
    #[allow(dead_code)]
    patches: Vec<(usize, String)>,
    /// Counter for generating unique labels (reserved for future use).
    #[allow(dead_code)]
    label_counter: u32,
}

impl Compiler {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            env: Env::new(),
            globals: Vec::new(),
            patches: Vec::new(),
            label_counter: 0,
        }
    }

    fn current_addr(&self) -> u32 {
        self.code.len() as u32
    }

    #[allow(dead_code)]
    fn fresh_label(&mut self) -> u32 {
        let l = self.label_counter;
        self.label_counter += 1;
        l
    }

    /// Emit instructions, returning the word address of the first word.
    fn emit(&mut self, instr: &Instr) -> u32 {
        let addr = self.current_addr();
        self.code.extend(instr.assemble());
        addr
    }

    /// Emit a jump to a not-yet-known address. Returns the address
    /// of the operand word so it can be patched later.
    fn emit_jump_placeholder(&mut self) -> usize {
        // operand(0) → PC
        let words = instr()
            .src(Unit::UNIT_ABS_OPERAND)
            .soperand(0)
            .dst(Unit::UNIT_PC)
            .assemble();
        let patch_idx = self.code.len() + 1; // operand is word 1
        self.code.extend(words);
        patch_idx
    }

    /// Emit a conditional jump to a not-yet-known address.
    fn emit_cond_jump_placeholder(&mut self) -> usize {
        let words = instr()
            .src(Unit::UNIT_ABS_OPERAND)
            .soperand(0)
            .dst(Unit::UNIT_PC_COND)
            .assemble();
        let patch_idx = self.code.len() + 1;
        self.code.extend(words);
        patch_idx
    }

    /// Emit a CALL to a not-yet-known address.
    #[allow(dead_code)]
    fn emit_call_placeholder(&mut self) -> usize {
        let words = instr()
            .src(Unit::UNIT_ABS_OPERAND)
            .soperand(0)
            .dst_call()
            .assemble();
        let patch_idx = self.code.len() + 1;
        self.code.extend(words);
        patch_idx
    }

    fn patch(&mut self, idx: usize, addr: u32) {
        self.code[idx] = addr;
    }

    /// Compile an expression. Result ends up in REG_ACC.
    fn compile_expr(&mut self, expr: &Expr, tail: bool) {
        match expr {
            Expr::Int(n) => {
                let v = *n as u32;
                if v < 256 {
                    self.emit(&instr()
                        .src(Unit::UNIT_ABS_IMMEDIATE).si(v as u8)
                        .dst_reg(REG_ACC));
                } else {
                    self.emit(&instr()
                        .src(Unit::UNIT_ABS_OPERAND).soperand(v)
                        .dst_reg(REG_ACC));
                }
                // Tag stays 0 (fixnum) — default from immediate
            }
            Expr::Bool(b) => {
                let v = if *b { 1u8 } else { 0 };
                self.emit(&instr()
                    .src(Unit::UNIT_ABS_IMMEDIATE).si(v)
                    .dst_reg(REG_ACC));
            }
            Expr::Nil => {
                // Nil = tag 3, value 0
                self.emit(&instr()
                    .src(Unit::UNIT_ABS_IMMEDIATE).si(0)
                    .dst_reg(REG_ACC));
                self.emit(&instr()
                    .src(Unit::UNIT_ABS_IMMEDIATE).si(Tag::Nil as u8)
                    .dst_reg_tag(REG_ACC));
            }
            Expr::Symbol(name) => {
                self.compile_var_ref(name);
            }
            Expr::List(elems) if elems.is_empty() => {
                // () is nil
                self.compile_expr(&Expr::Nil, tail);
            }
            Expr::List(elems) => {
                let head = &elems[0];
                match head {
                    Expr::Symbol(s) if s == "quote" => {
                        self.compile_quote(&elems[1]);
                    }
                    Expr::Symbol(s) if s == "if" => {
                        self.compile_if(elems, tail);
                    }
                    Expr::Symbol(s) if s == "define" => {
                        self.compile_define(elems);
                    }
                    Expr::Symbol(s) if s == "lambda" => {
                        self.compile_lambda(elems);
                    }
                    Expr::Symbol(s) if s == "let" => {
                        self.compile_let(elems, tail);
                    }
                    Expr::Symbol(s) if s == "begin" => {
                        self.compile_begin(&elems[1..], tail);
                    }
                    Expr::Symbol(s) if is_primitive(s) => {
                        self.compile_primitive(s, &elems[1..]);
                    }
                    _ => {
                        self.compile_call(elems, tail);
                    }
                }
            }
        }
    }

    fn compile_var_ref(&mut self, name: &str) {
        if let Some((depth, slot)) = self.env.lookup(name) {
            // Load environment frame into a temp register, chase parent chain
            self.emit(&instr().src_reg(REG_ENV).dst_reg(REG_TEMP_BASE));
            for _ in 0..depth {
                // Chase parent pointer: deref(temp, 0) → temp
                self.emit(&instr()
                    .src_deref(REG_TEMP_BASE, 0)
                    .dst_reg(REG_TEMP_BASE));
            }
            // Load slot: deref(temp, slot+1) → ACC
            let offset = (slot + 1) as u8;
            assert!(offset < 8, "Frame slot {} exceeds DEREF offset limit (max 6 vars per frame)", slot);
            self.emit(&instr()
                .src_deref(REG_TEMP_BASE, offset)
                .dst_reg(REG_ACC));
        } else if let Some(global) = self.globals.iter().find(|g| g.name == name).cloned() {
            // Global function reference — load as a closure with no env
            // For now, just load the code address as a lambda-tagged value
            self.emit_make_closure(global.addr, false);
        } else {
            panic!("Undefined variable: {}", name);
        }
    }

    fn compile_quote(&mut self, expr: &Expr) {
        match expr {
            Expr::Int(n) => self.compile_expr(&Expr::Int(*n), false),
            Expr::Bool(b) => self.compile_expr(&Expr::Bool(*b), false),
            Expr::Nil => self.compile_expr(&Expr::Nil, false),
            Expr::Symbol(s) => {
                // Quoted symbol — intern it as an integer index
                // For now, use a simple hash
                let idx = symbol_index(s);
                if idx < 256 {
                    self.emit(&instr()
                        .src(Unit::UNIT_ABS_IMMEDIATE).si(idx as u8)
                        .dst_reg(REG_ACC));
                } else {
                    self.emit(&instr()
                        .src(Unit::UNIT_ABS_OPERAND).soperand(idx)
                        .dst_reg(REG_ACC));
                }
                self.emit(&instr()
                    .src(Unit::UNIT_ABS_IMMEDIATE).si(Tag::Symbol as u8)
                    .dst_reg_tag(REG_ACC));
            }
            Expr::List(elems) => {
                // Build a list at runtime via cons cells
                // (quote (a b c)) → (cons 'a (cons 'b (cons 'c nil)))
                // Build from right to left so cdr is available
                self.compile_expr(&Expr::Nil, false);
                self.emit(&instr()
                    .src_reg(REG_ACC)
                    .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_EVAL));
                for elem in elems.iter().rev() {
                    // cdr is on stack, compile car
                    self.compile_quote(elem);
                    // ACC = car, stack top = cdr
                    // Allocate cons: alloc_ptr[CONS] → temp
                    self.emit(&instr()
                        .src_alloc_ptr(Tag::Cons as u8)
                        .dst_reg(REG_TEMP_BASE));
                    // Store car
                    self.emit(&instr()
                        .src_reg(REG_ACC)
                        .dst(Unit::UNIT_ALLOC));
                    // Store cdr (pop from stack)
                    self.emit(&instr()
                        .src_pop(STACK_EVAL)
                        .dst(Unit::UNIT_ALLOC));
                    // Push new cons pointer for next iteration
                    self.emit(&instr()
                        .src_reg(REG_TEMP_BASE)
                        .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_EVAL));
                }
                // Final result is on stack
                self.emit(&instr()
                    .src_pop(STACK_EVAL)
                    .dst_reg(REG_ACC));
            }
        }
    }

    fn compile_if(&mut self, elems: &[Expr], tail: bool) {
        // (if test then else)
        assert!(elems.len() == 3 || elems.len() == 4, "if requires 2 or 3 args");
        let has_else = elems.len() == 4;

        // Compile test → ACC
        self.compile_expr(&elems[1], false);
        // Set cond from ACC
        self.emit(&instr()
            .src_reg(REG_ACC)
            .dst(Unit::UNIT_COND));
        // Branch to else if cond is clear
        let else_patch = self.emit_cond_jump_placeholder();
        // Negate: we want to jump if FALSE. PC_COND jumps if SET.
        // So we need to invert: use the cond value directly.
        // Actually: PC_COND jumps if cond is set. We set cond from
        // the test value. If test is truthy (nonzero), cond=1, we
        // want to execute 'then'. So jump over 'then' to 'else'
        // when cond is CLEAR. But PC_COND only jumps when SET.
        //
        // Fix: emit "jump to else" as unconditional after setting
        // cond, but use predication. Or invert the logic.
        //
        // Simpler: compile_expr(test), set cond, then:
        //   - if cond clear → jump to else
        // We don't have "jump if clear" as a unit. But we have
        // predication. Let me restructure:

        // Actually let me redo this. We have two options:
        // 1. EQL test with 0 → if result is truthy (test was 0 = false), jump to else
        // 2. Use predicated jump

        // Let me use approach: test → cond, then jump-if-set to then_label,
        // fall through to else.

        // Redo: remove the placeholder we just emitted and do it properly.
        // Back up: we already emitted the cond_jump_placeholder. Let's
        // use it as "jump to then" when cond is set.
        // But that means else is the fall-through...
        // (if test then else):
        //   compile(test) → ACC
        //   ACC → cond
        //   cond_jump → then_label        [jump if truthy]
        //   compile(else) → ACC
        //   jump → end_label
        //   then_label:
        //   compile(then) → ACC
        //   end_label:

        // Actually that's more code. Standard approach is:
        //   compile(test) → ACC
        //   if ACC == 0, jump to else_label
        //   compile(then) → ACC
        //   jump → end_label
        //   else_label:
        //   compile(else) → ACC
        //   end_label:

        // To get "jump if zero": compare ACC with 0, set cond,
        // then PC_COND jumps if set. So:
        //   ACC → alu[0].left
        //   0 → alu[0].right
        //   EQL → alu[0].operator
        //   alu[0].result → cond   (cond = 1 if ACC == 0)
        //   addr → pc_cond         (jump if ACC was zero = falsy)

        // Hmm, that's 5 instructions just for the branch. Let me
        // think about this differently.
        //
        // Actually: COND as destination sets cond = (value != 0).
        // So after `ACC → cond`, cond=1 means truthy.
        // We want to jump to else when cond=0 (falsy).
        // PC_COND jumps when cond=1.
        //
        // So the pattern is:
        //   ACC → cond
        //   then_addr → pc_cond       ; jump to then if truthy
        //   compile(else)              ; fall through = falsy
        //   end_addr → PC             ; skip over then
        //   then:
        //   compile(then)
        //   end:

        // This is what we want. Let me redo. We already emitted
        // ACC → cond and a cond_jump_placeholder. The placeholder
        // is a PC_COND jump. So it jumps to then_label when truthy.
        // Let's use it that way.

        // Back up and re-emit properly. The code we've emitted so far:
        //   compile(test) → ACC
        //   ACC → cond              (emitted above)
        //   operand(0) → PC_COND   (placeholder, jumps if cond=1 = truthy)
        //
        // So the placeholder should jump to then_label. Fall through = else.

        // Compile else (fall-through when falsy)
        if has_else {
            self.compile_expr(&elems[3], tail);
        } else {
            self.compile_expr(&Expr::Nil, tail);
        }
        let end_patch = self.emit_jump_placeholder();

        // Then label
        let then_addr = self.current_addr();
        self.patch(else_patch, then_addr); // patch the PC_COND to jump here
        self.compile_expr(&elems[2], tail);

        // End label
        let end_addr = self.current_addr();
        self.patch(end_patch, end_addr);
    }

    fn compile_define(&mut self, elems: &[Expr]) {
        // (define name expr) or (define (name args...) body)
        match &elems[1] {
            Expr::Symbol(_name) => {
                // (define name expr) — compile expr, make it a global
                // For now: only support function values
                self.compile_expr(&elems[2], false);
                // Store in a global slot — we'll handle this later
                // For now just track the name
                panic!("define of non-function globals not yet supported; use (define (name args...) body)");
            }
            Expr::List(sig) => {
                // (define (name args...) body)
                let name = match &sig[0] {
                    Expr::Symbol(s) => s.clone(),
                    _ => panic!("define: expected function name"),
                };
                let params: Vec<String> = sig[1..]
                    .iter()
                    .map(|e| match e {
                        Expr::Symbol(s) => s.clone(),
                        _ => panic!("define: expected parameter name"),
                    })
                    .collect();
                let arity = params.len();

                // Jump over the function body (it's inline in the code stream)
                let skip_patch = self.emit_jump_placeholder();

                let func_addr = self.current_addr();
                self.globals.push(GlobalDef {
                    name: name.clone(),
                    addr: func_addr,
                    arity,
                });

                // Function prologue: allocate frame, pop args from eval stack
                self.compile_function_prologue(&params);

                // Compile body in the new scope
                let body = if elems.len() > 3 {
                    // Multiple body forms
                    Expr::List(
                        std::iter::once(Expr::Symbol("begin".into()))
                            .chain(elems[2..].iter().cloned())
                            .collect(),
                    )
                } else {
                    elems[2].clone()
                };
                self.compile_expr(&body, true); // tail position

                // Function epilogue: restore env, return
                self.compile_function_epilogue();

                let after_addr = self.current_addr();
                self.patch(skip_patch, after_addr);
            }
            _ => panic!("define: expected name or (name args...)"),
        }
    }

    fn compile_lambda(&mut self, elems: &[Expr]) {
        // (lambda (args...) body)
        assert!(elems.len() >= 3, "lambda requires params and body");
        let params: Vec<String> = match &elems[1] {
            Expr::List(ps) => ps
                .iter()
                .map(|e| match e {
                    Expr::Symbol(s) => s.clone(),
                    _ => panic!("lambda: expected parameter name"),
                })
                .collect(),
            _ => panic!("lambda: expected parameter list"),
        };

        // Jump over the lambda body
        let skip_patch = self.emit_jump_placeholder();

        let func_addr = self.current_addr();

        // Function prologue
        self.compile_function_prologue(&params);

        // Compile body
        let body = if elems.len() > 3 {
            Expr::List(
                std::iter::once(Expr::Symbol("begin".into()))
                    .chain(elems[2..].iter().cloned())
                    .collect(),
            )
        } else {
            elems[2].clone()
        };
        self.compile_expr(&body, true);

        // Function epilogue
        self.compile_function_epilogue();

        let after_addr = self.current_addr();
        self.patch(skip_patch, after_addr);

        // Create closure: allocate [code_addr, env_ptr] tagged as Lambda
        self.emit_make_closure(func_addr, true);
    }

    /// Emit code to construct a closure value in REG_ACC.
    /// If `capture_env` is true, the closure captures the current REG_ENV.
    /// Otherwise, the env slot is nil (for top-level functions).
    fn emit_make_closure(&mut self, code_addr: u32, capture_env: bool) {
        // alloc_ptr[Lambda] → ACC (tagged closure pointer)
        self.emit(&instr()
            .src_alloc_ptr(Tag::Lambda as u8)
            .dst_reg(REG_ACC));
        // Store code address
        if code_addr < 256 {
            self.emit(&instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(code_addr as u8)
                .dst(Unit::UNIT_ALLOC));
        } else {
            self.emit(&instr()
                .src(Unit::UNIT_ABS_OPERAND).soperand(code_addr)
                .dst(Unit::UNIT_ALLOC));
        }
        // Store env pointer
        if capture_env {
            self.emit(&instr()
                .src_reg(REG_ENV)
                .dst(Unit::UNIT_ALLOC));
        } else {
            // No captured environment — store nil
            self.emit(&instr()
                .src(Unit::UNIT_ABS_IMMEDIATE).si(0)
                .dst(Unit::UNIT_ALLOC));
        }
    }

    fn compile_function_prologue(&mut self, params: &[String]) {
        let frame_size = params.len() + 1; // +1 for parent pointer
        assert!(frame_size <= 8, "Frame too large (max 7 params per function)");

        // Allocate frame: alloc_ptr[Fixnum] → temp
        // (frames aren't tagged — they're internal structure)
        self.emit(&instr()
            .src_alloc_ptr(Tag::Fixnum as u8)
            .dst_reg(REG_TEMP_BASE));
        // Store parent pointer (current env)
        self.emit(&instr()
            .src_reg(REG_ENV)
            .dst(Unit::UNIT_ALLOC));
        // Pop args from eval stack into frame slots
        // Args are pushed left-to-right by caller, so pop in reverse
        // to get them in the right slots.
        // Actually: caller pushes left-to-right, so last arg is on top.
        // We want slot 0 = first arg. So pop into slots in reverse order.
        for _i in (0..params.len()).rev() {
            self.emit(&instr()
                .src_pop(STACK_EVAL)
                .dst(Unit::UNIT_ALLOC));
        }
        // Set ENV to new frame
        self.emit(&instr()
            .src_reg(REG_TEMP_BASE)
            .dst_reg(REG_ENV));

        self.env.push_scope(params.to_vec());
    }

    fn compile_function_epilogue(&mut self) {
        self.env.pop_scope();
        // Restore caller's env from env stack
        self.emit(&instr()
            .src_pop(STACK_ENV)
            .dst_reg(REG_ENV));
        // Return: pop stack[1] → PC
        self.emit(&instr()
            .src_pop(STACK_RETURN)
            .dst(Unit::UNIT_PC));
    }

    fn compile_let(&mut self, elems: &[Expr], tail: bool) {
        // (let ((x 1) (y 2)) body)
        assert!(elems.len() >= 3, "let requires bindings and body");
        let bindings = match &elems[1] {
            Expr::List(bs) => bs,
            _ => panic!("let: expected binding list"),
        };

        let mut var_names = Vec::new();
        // Evaluate all binding values and push to eval stack
        for binding in bindings {
            let (name, val) = match binding {
                Expr::List(pair) if pair.len() == 2 => {
                    let name = match &pair[0] {
                        Expr::Symbol(s) => s.clone(),
                        _ => panic!("let: expected variable name"),
                    };
                    (name, &pair[1])
                }
                _ => panic!("let: expected (name value) binding"),
            };
            self.compile_expr(val, false);
            self.emit(&instr()
                .src_reg(REG_ACC)
                .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_EVAL));
            var_names.push(name);
        }

        let frame_size = var_names.len() + 1;
        assert!(frame_size <= 8, "let frame too large (max 7 bindings)");

        // Allocate frame
        self.emit(&instr()
            .src_alloc_ptr(Tag::Fixnum as u8)
            .dst_reg(REG_TEMP_BASE));
        // Store parent (current env)
        self.emit(&instr()
            .src_reg(REG_ENV)
            .dst(Unit::UNIT_ALLOC));
        // Pop values into frame slots (reverse order since stack is LIFO)
        for _ in (0..var_names.len()).rev() {
            self.emit(&instr()
                .src_pop(STACK_EVAL)
                .dst(Unit::UNIT_ALLOC));
        }
        // Set ENV
        self.emit(&instr()
            .src_reg(REG_TEMP_BASE)
            .dst_reg(REG_ENV));

        self.env.push_scope(var_names);

        // Compile body
        let body = if elems.len() > 3 {
            Expr::List(
                std::iter::once(Expr::Symbol("begin".into()))
                    .chain(elems[2..].iter().cloned())
                    .collect(),
            )
        } else {
            elems[2].clone()
        };
        self.compile_expr(&body, tail);

        self.env.pop_scope();

        // Restore env from parent pointer in frame
        // Actually, we didn't push env to a stack here. For let,
        // we can restore env from the frame's parent pointer.
        self.emit(&instr()
            .src_deref(REG_ENV, 0)
            .dst_reg(REG_ENV));
    }

    fn compile_begin(&mut self, exprs: &[Expr], tail: bool) {
        if exprs.is_empty() {
            self.compile_expr(&Expr::Nil, tail);
            return;
        }
        for (i, expr) in exprs.iter().enumerate() {
            let is_last = i == exprs.len() - 1;
            self.compile_expr(expr, tail && is_last);
        }
    }

    fn compile_primitive(&mut self, name: &str, args: &[Expr]) {
        match name {
            "+" | "-" | "*" | "=" | ">" | "<" => {
                self.compile_binary_arith(name, args);
            }
            "cons" => {
                assert_eq!(args.len(), 2, "cons requires 2 args");
                // Compile car
                self.compile_expr(&args[0], false);
                self.emit(&instr()
                    .src_reg(REG_ACC)
                    .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_EVAL));
                // Compile cdr
                self.compile_expr(&args[1], false);
                // ACC = cdr, stack top = car
                // Allocate cons cell
                self.emit(&instr()
                    .src_alloc_ptr(Tag::Cons as u8)
                    .dst_reg(REG_TEMP_BASE));
                // Pop car and store
                self.emit(&instr()
                    .src_pop(STACK_EVAL)
                    .dst(Unit::UNIT_ALLOC));
                // Store cdr (ACC)
                self.emit(&instr()
                    .src_reg(REG_ACC)
                    .dst(Unit::UNIT_ALLOC));
                // Result is the cons pointer
                self.emit(&instr()
                    .src_reg(REG_TEMP_BASE)
                    .dst_reg(REG_ACC));
            }
            "car" => {
                assert_eq!(args.len(), 1, "car requires 1 arg");
                self.compile_expr(&args[0], false);
                // ACC has cons pointer — deref offset 0
                self.emit(&instr()
                    .src_reg(REG_ACC)
                    .dst_reg(REG_TEMP_BASE));
                self.emit(&instr()
                    .src_deref(REG_TEMP_BASE, 0)
                    .dst_reg(REG_ACC));
            }
            "cdr" => {
                assert_eq!(args.len(), 1, "cdr requires 1 arg");
                self.compile_expr(&args[0], false);
                self.emit(&instr()
                    .src_reg(REG_ACC)
                    .dst_reg(REG_TEMP_BASE));
                self.emit(&instr()
                    .src_deref(REG_TEMP_BASE, 1)
                    .dst_reg(REG_ACC));
            }
            "null?" => {
                assert_eq!(args.len(), 1, "null? requires 1 arg");
                self.compile_expr(&args[0], false);
                // Check if tag == Nil
                self.emit(&instr()
                    .src_reg(REG_ACC)
                    .dst_tag_cmp(Tag::Nil as u8));
                // cond is now set if nil. Move cond to ACC as boolean.
                self.emit(&instr()
                    .src(Unit::UNIT_COND)
                    .dst_reg(REG_ACC));
            }
            "eq?" => {
                assert_eq!(args.len(), 2, "eq? requires 2 args");
                // Simple pointer/value equality on the value portion
                self.compile_binary_arith("=", args);
            }
            "not" => {
                assert_eq!(args.len(), 1, "not requires 1 arg");
                self.compile_expr(&args[0], false);
                // ACC → cond (truthy check)
                self.emit(&instr()
                    .src_reg(REG_ACC)
                    .dst(Unit::UNIT_COND));
                // Invert: set ACC to 1 if cond was 0, else 0
                self.emit(&instr()
                    .src(Unit::UNIT_ABS_IMMEDIATE).si(1)
                    .dst_reg(REG_ACC));
                self.emit(&instr()
                    .src(Unit::UNIT_ABS_IMMEDIATE).si(0)
                    .dst_reg(REG_ACC)
                    .predicate_if_set());
            }
            _ => panic!("Unknown primitive: {}", name),
        }
    }

    fn compile_binary_arith(&mut self, op: &str, args: &[Expr]) {
        assert_eq!(args.len(), 2, "{} requires 2 args", op);
        // Compile left
        self.compile_expr(&args[0], false);
        self.emit(&instr()
            .src_reg(REG_ACC)
            .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_EVAL));
        // Compile right
        self.compile_expr(&args[1], false);
        // ACC = right, stack = left
        // Pop left → alu[0].left
        self.emit(&instr()
            .src_pop(STACK_EVAL)
            .dst(Unit::UNIT_ALU_LEFT).di(0));
        // ACC (right) → alu[0].right
        self.emit(&instr()
            .src_reg(REG_ACC)
            .dst(Unit::UNIT_ALU_RIGHT).di(0));
        // Set operator
        let alu_op = match op {
            "+" => ALUOp::ALU_ADD,
            "-" => ALUOp::ALU_SUB,
            "*" => ALUOp::ALU_MUL,
            "=" => ALUOp::ALU_EQL,
            ">" => ALUOp::ALU_GT,
            "<" => ALUOp::ALU_LT,
            _ => panic!("Unknown arith op: {}", op),
        };
        self.emit(&instr()
            .src(Unit::UNIT_ABS_IMMEDIATE).si(alu_op as u8)
            .dst(Unit::UNIT_ALU_OPERATOR).di(0));
        // Read result
        self.emit(&instr()
            .src(Unit::UNIT_ALU_RESULT).si(0)
            .dst_reg(REG_ACC));
    }

    fn compile_call(&mut self, elems: &[Expr], tail: bool) {
        let func_expr = &elems[0];
        let args = &elems[1..];

        // Push args left-to-right onto eval stack
        for arg in args {
            self.compile_expr(arg, false);
            self.emit(&instr()
                .src_reg(REG_ACC)
                .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_EVAL));
        }

        // Check if this is a direct call to a known global
        if let Expr::Symbol(name) = func_expr {
            if let Some(global) = self.globals.iter().find(|g| g.name == *name).cloned() {
                // Direct call to known function
                if !tail {
                    // Save current env
                    self.emit(&instr()
                        .src_reg(REG_ENV)
                        .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_ENV));
                    // Call
                    self.emit(&instr()
                        .src(Unit::UNIT_ABS_OPERAND).soperand(global.addr)
                        .dst_call());
                } else {
                    // Tail call: don't push return addr, don't save env
                    // Pop the current return address first? No — for tail
                    // calls to work, we keep the existing return address
                    // on stack 1. The callee will return to our caller.
                    // But we do need to restore our caller's env before
                    // the tail call, since the callee will push its own.
                    // Actually: the callee's prologue pushes nothing to
                    // env stack — it's the *caller* that pushes env before
                    // calling. In a tail call, we ARE the caller of the
                    // next function, so we push env. But the current
                    // function's env was pushed by OUR caller. Hmm.
                    //
                    // For now, just do a regular call for tail position too.
                    // TCO requires more careful env management.
                    self.emit(&instr()
                        .src_reg(REG_ENV)
                        .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_ENV));
                    self.emit(&instr()
                        .src(Unit::UNIT_ABS_OPERAND).soperand(global.addr)
                        .dst_call());
                }
                return;
            }
        }

        // General case: evaluate function expression, then call via closure
        self.compile_expr(func_expr, false);
        // ACC = closure (Lambda-tagged pointer to [code_addr, env_ptr])
        // Save closure in temp
        self.emit(&instr()
            .src_reg(REG_ACC)
            .dst_reg(REG_TEMP_BASE));
        // Save current env
        self.emit(&instr()
            .src_reg(REG_ENV)
            .dst(Unit::UNIT_STACK_PUSH_POP).di(STACK_ENV));
        // Load closure's env into REG_ENV
        self.emit(&instr()
            .src_deref(REG_TEMP_BASE, 1)
            .dst_reg(REG_ENV));
        // Load code address and call
        self.emit(&instr()
            .src_deref(REG_TEMP_BASE, 0)
            .dst_reg(REG_TEMP_BASE + 1));
        self.emit(&instr()
            .src_reg(REG_TEMP_BASE + 1)
            .dst_call());
    }
}

fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "+" | "-" | "*" | "=" | ">" | "<" | "cons" | "car" | "cdr" | "null?" | "eq?" | "not"
    )
}

/// Simple symbol interning: deterministic index from name.
fn symbol_index(name: &str) -> u32 {
    // Use a simple hash for now. A real implementation would use
    // a proper intern table shared between compiler and runtime.
    let mut h: u32 = 5381;
    for b in name.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u32);
    }
    h & 0x7FFF_FFFF // keep positive
}

/// Compile a Lisp program (source string) to TTA machine code.
pub fn compile(source: &str) -> Result<Program, String> {
    let ast = crate::parser::parse(source)?;
    let mut compiler = Compiler::new();

    // First pass: collect top-level defines.
    // We need to do this in order, since defines produce code inline.
    compiler.compile_expr(&ast, false);

    // Emit halt (none → none)
    compiler.emit(&instr());

    Ok(Program {
        entry: 0,
        code: compiler.code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_integer() {
        let prog = compile("42").unwrap();
        assert!(!prog.code.is_empty());
        // Should be: imm(42) → reg[1], then halt
        assert!(prog.code.len() >= 2);
    }

    #[test]
    fn test_compile_addition() {
        let prog = compile("(+ 1 2)").unwrap();
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn test_compile_nested_arith() {
        let prog = compile("(+ (* 2 3) 4)").unwrap();
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn test_compile_if() {
        let prog = compile("(if 1 2 3)").unwrap();
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn test_compile_cons_car_cdr() {
        let prog = compile("(car (cons 1 2))").unwrap();
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn test_compile_let() {
        let prog = compile("(let ((x 10) (y 20)) (+ x y))").unwrap();
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn test_compile_define_and_call() {
        let prog = compile("(begin (define (add a b) (+ a b)) (add 3 4))").unwrap();
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn test_compile_lambda() {
        let prog = compile("(let ((f (lambda (x) (+ x 1)))) (f 10))").unwrap();
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn test_compile_null_check() {
        let prog = compile("(null? ())").unwrap();
        assert!(!prog.code.is_empty());
    }

    #[test]
    fn test_compile_closure() {
        let prog = compile("(let ((a 5)) (let ((f (lambda (x) (+ x a)))) (f 10)))").unwrap();
        assert!(!prog.code.is_empty());
    }
}
