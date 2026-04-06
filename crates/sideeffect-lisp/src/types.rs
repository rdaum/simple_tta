/// Tag assignments for the 4-bit sidecar tag (16 types available).
///
/// These values correspond directly to the hardware tag bits [35:32]
/// in the 36-bit data path. The compiler emits these as immediates
/// to ALLOC_PTR, TAG_CMP, and REG_TAG instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Tag {
    Fixnum  = 0,  // Unboxed integer — default tag for arithmetic results
    Cons    = 1,  // Pointer to 2-word cell: [car][cdr]
    Symbol  = 2,  // Index into compile-time symbol table
    Nil     = 3,  // Empty list / false — value bits ignored
    Lambda  = 4,  // Pointer to closure: [code_addr][env_ptr]
    Builtin = 5,  // Index into primitive function dispatch table
    // 6-15 reserved for future types (string, vector, char, float, ...)
}

/// Expr is the AST produced by the parser and consumed by the compiler.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i32),
    Symbol(String),
    Bool(bool),
    List(Vec<Expr>),
    Nil,
}

impl Expr {
    pub fn is_symbol(&self, name: &str) -> bool {
        matches!(self, Expr::Symbol(s) if s == name)
    }
}

/// Register allocation conventions.
///
/// The compiler uses a small set of dedicated registers for runtime
/// bookkeeping. General-purpose temporaries start at REG_TEMP_BASE.
pub mod regs {
    /// Environment pointer — points to the current lexical frame.
    pub const REG_ENV: u8 = 0;
    /// General-purpose accumulator / return value.
    pub const REG_ACC: u8 = 1;
    /// Temporary registers start here (2..31).
    pub const REG_TEMP_BASE: u8 = 2;
}

/// Stack assignments.
pub mod stacks {
    /// Value/eval stack — intermediate computation results.
    pub const STACK_EVAL: u8 = 0;
    /// Return address stack — managed by CALL/RET.
    pub const STACK_RETURN: u8 = 1;
    /// Saved environment pointers for call/return.
    pub const STACK_ENV: u8 = 2;
}
