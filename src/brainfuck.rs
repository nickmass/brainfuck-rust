extern crate libc;

use std::collections::VecDeque;
use std::io::Read;

pub struct Brainfuck<'a> {
    program: Program<'a>,
}

impl<'a> Brainfuck<'a> {
    pub fn parse<T>(
        reader: T,
        file_name: &'a str,
        directory: &'a str,
    ) -> Result<Brainfuck<'a>, ParseError<'a>>
    where
        T: Read,
    {
        let mut symbols = VecDeque::new();
        let mut line = 1;
        let mut column = 1;
        for byte in reader.bytes() {
            let debug = DebugInfo {
                directory: directory,
                file: file_name,
                line: line,
                column: column,
            };
            if byte.is_err() {
                break;
            }
            let byte = byte.unwrap();
            match byte {
                b'>' => symbols.push_back(Symbol::IncPtr(debug)),
                b'<' => symbols.push_back(Symbol::DecPtr(debug)),
                b'+' => symbols.push_back(Symbol::Increment(debug)),
                b'-' => symbols.push_back(Symbol::Decrement(debug)),
                b'.' => symbols.push_back(Symbol::Output(debug)),
                b',' => symbols.push_back(Symbol::Input(debug)),
                b'[' => symbols.push_back(Symbol::OpenBlock(debug)),
                b']' => symbols.push_back(Symbol::CloseBlock(debug)),
                b'\n' => {
                    line += 1;
                    column = 0;
                }
                _ => (),
            }
            column += 1;
        }

        let ast = Ast::parse(symbols);

        ast.map(|mut x| {
            x.optimize();
            Brainfuck {
                program: Program::new(x, 100000),
            }
        })
    }

    pub fn exec(&'a self) -> Result<(), ExecError<'a>> {
        self.program.exec()
    }

    pub fn gen_ir(&self) -> String {
        self.program.gen_ir()
    }
}

#[derive(Debug)]
enum Symbol<'a> {
    IncPtr(DebugInfo<'a>),
    DecPtr(DebugInfo<'a>),
    Increment(DebugInfo<'a>),
    Decrement(DebugInfo<'a>),
    Output(DebugInfo<'a>),
    Input(DebugInfo<'a>),
    OpenBlock(DebugInfo<'a>),
    CloseBlock(DebugInfo<'a>),
}

#[derive(Debug)]
pub struct DebugInfo<'a> {
    pub directory: &'a str,
    pub file: &'a str,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug)]
pub enum ParseError<'a> {
    UnmatchedLoop(DebugInfo<'a>),
}

#[derive(Debug)]
enum ParseResult<'a> {
    UnmatchedLoop(DebugInfo<'a>),
    CloseLoop(DebugInfo<'a>),
    Ok,
    Eof,
}

#[derive(Debug)]
enum Node<'a> {
    Loop(VecDeque<Node<'a>>, DebugInfo<'a>),
    IncPtr(usize, DebugInfo<'a>),
    DecPtr(usize, DebugInfo<'a>),
    Increment(u8, DebugInfo<'a>),
    Decrement(u8, DebugInfo<'a>),
    Output(DebugInfo<'a>),
    Input(DebugInfo<'a>),
}

#[derive(Debug)]
struct Ast<'a> {
    nodes: VecDeque<Node<'a>>,
}

impl<'a> Ast<'a> {
    fn parse(mut symbols: VecDeque<Symbol<'a>>) -> Result<Ast<'a>, ParseError<'a>> {
        let mut ast = Ast {
            nodes: VecDeque::new(),
        };

        while {
            match Self::add_node(&mut ast.nodes, &mut symbols) {
                ParseResult::Ok => true,
                ParseResult::UnmatchedLoop(d) => return Err(ParseError::UnmatchedLoop(d)),
                ParseResult::CloseLoop(d) => return Err(ParseError::UnmatchedLoop(d)),
                ParseResult::Eof => false,
            }
        } {}

        Ok(ast)
    }

    fn add_node(
        nodes: &mut VecDeque<Node<'a>>,
        symbols: &mut VecDeque<Symbol<'a>>,
    ) -> ParseResult<'a> {
        let symbol = symbols.pop_front();

        match symbol {
            Some(Symbol::OpenBlock(d)) => {
                let mut loop_body = VecDeque::new();
                while {
                    match Self::add_node(&mut loop_body, symbols) {
                        ParseResult::UnmatchedLoop(d) => {
                            return ParseResult::UnmatchedLoop(d);
                        }
                        ParseResult::CloseLoop(_) => false,
                        ParseResult::Ok => true,
                        ParseResult::Eof => {
                            return ParseResult::UnmatchedLoop(d);
                        }
                    }
                } {}
                nodes.push_back(Node::Loop(loop_body, d));
                ParseResult::Ok
            }
            Some(Symbol::CloseBlock(d)) => ParseResult::CloseLoop(d),
            Some(Symbol::IncPtr(d)) => {
                nodes.push_back(Node::IncPtr(1, d));
                ParseResult::Ok
            }
            Some(Symbol::DecPtr(d)) => {
                nodes.push_back(Node::DecPtr(1, d));
                ParseResult::Ok
            }
            Some(Symbol::Increment(d)) => {
                nodes.push_back(Node::Increment(1, d));
                ParseResult::Ok
            }
            Some(Symbol::Decrement(d)) => {
                nodes.push_back(Node::Decrement(1, d));
                ParseResult::Ok
            }
            Some(Symbol::Output(d)) => {
                nodes.push_back(Node::Output(d));
                ParseResult::Ok
            }
            Some(Symbol::Input(d)) => {
                nodes.push_back(Node::Input(d));
                ParseResult::Ok
            }
            None => ParseResult::Eof,
        }
    }

    fn optimize(&mut self) {
        let mut opt_nodes = VecDeque::new();
        Self::optimize_nodes(&mut opt_nodes, &mut self.nodes);
        self.nodes = opt_nodes;
    }

    fn optimize_nodes(opt_nodes: &mut VecDeque<Node<'a>>, nodes: &mut VecDeque<Node<'a>>) {
        while let Some(node) = nodes.pop_front() {
            match node {
                Node::Loop(mut n, d) => {
                    let mut loop_body = VecDeque::new();
                    Self::optimize_nodes(&mut loop_body, &mut n);
                    opt_nodes.push_back(Node::Loop(loop_body, d));
                }
                Node::IncPtr(v, d) => {
                    let mut value = v;
                    while let Some(&Node::IncPtr(v, _)) = nodes.front() {
                        value = value.wrapping_add(v);
                        nodes.pop_front();
                    }
                    opt_nodes.push_back(Node::IncPtr(value, d));
                }
                Node::DecPtr(v, d) => {
                    let mut value = v;
                    while let Some(&Node::DecPtr(v, _)) = nodes.front() {
                        value = value.wrapping_add(v);
                        nodes.pop_front();
                    }
                    opt_nodes.push_back(Node::DecPtr(value, d));
                }
                Node::Increment(v, d) => {
                    let mut value = v;
                    while let Some(&Node::Increment(v, _)) = nodes.front() {
                        value = value.wrapping_add(v);
                        nodes.pop_front();
                    }
                    opt_nodes.push_back(Node::Increment(value, d));
                }
                Node::Decrement(v, d) => {
                    let mut value = v;
                    while let Some(&Node::Decrement(v, _)) = nodes.front() {
                        value = value.wrapping_add(v);
                        nodes.pop_front();
                    }
                    opt_nodes.push_back(Node::Decrement(value, d));
                }
                Node::Input(d) => {
                    opt_nodes.push_back(Node::Input(d));
                }
                Node::Output(d) => {
                    opt_nodes.push_back(Node::Output(d));
                }
            }
        }
    }
}

struct Program<'a> {
    ast: Ast<'a>,
    mem_size: usize,
}

struct ProgramState {
    ptr: usize,
    mem: Vec<u8>,
    mem_size: usize,
}

impl ProgramState {
    fn new(mem_size: usize) -> ProgramState {
        let mut mem = Vec::with_capacity(mem_size);
        for _ in 0..mem_size {
            mem.push(0);
        }

        ProgramState {
            ptr: mem_size / 2,
            mem: mem,
            mem_size: mem_size,
        }
    }

    fn is_oob(&self) -> bool {
        self.ptr >= self.mem_size
    }
}

struct IrState {
    next_label: i32,
}

impl IrState {
    fn new() -> IrState {
        IrState { next_label: 0 }
    }

    fn ident(&mut self) -> String {
        self.next_label += 1;
        format!("%i{}", self.next_label)
    }

    fn label(&mut self) -> String {
        self.next_label += 1;
        format!("l{}", self.next_label)
    }
}

pub enum ExecError<'a> {
    OutOfBounds(&'a DebugInfo<'a>),
}

impl<'a> Program<'a> {
    fn new(ast: Ast<'a>, mem_size: usize) -> Program<'a> {
        let prog = Program {
            ast: ast,
            mem_size: mem_size,
        };

        prog
    }

    fn exec(&'a self) -> Result<(), ExecError<'a>> {
        let mut state = ProgramState::new(self.mem_size);
        Self::exec_nodes(&mut state, &self.ast.nodes)
    }

    fn exec_nodes(
        state: &mut ProgramState,
        nodes: &'a VecDeque<Node<'a>>,
    ) -> Result<(), ExecError<'a>> {
        for node in nodes {
            match node {
                &Node::IncPtr(v, _) => state.ptr = state.ptr.wrapping_add(v),
                &Node::DecPtr(v, _) => state.ptr = state.ptr.wrapping_sub(v),
                &Node::Increment(v, ref d) => {
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d));
                    }
                    let val = state.mem[state.ptr].wrapping_add(v);
                    state.mem[state.ptr] = val;
                }
                &Node::Decrement(v, ref d) => {
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d));
                    }
                    let val = state.mem[state.ptr].wrapping_sub(v);
                    state.mem[state.ptr] = val;
                }
                &Node::Output(ref d) => {
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d));
                    }
                    print!("{}", state.mem[state.ptr] as char)
                }
                &Node::Input(ref d) => {
                    let val = unsafe { libc::getchar() };
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d));
                    }
                    state.mem[state.ptr] = val as u8;
                }
                &Node::Loop(ref nodes, ref d) => {
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d));
                    }
                    while state.mem[state.ptr] != 0 {
                        match Self::exec_nodes(state, nodes) {
                            Err(e) => return Err(e),
                            _ => (),
                        }
                        if state.is_oob() {
                            return Err(ExecError::OutOfBounds(d));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn gen_ir(&self) -> String {
        let mut ir = String::new();
        let mut ir_state = IrState::new();
        let prelude = format!(
            r"
@mem = private global [{} x i8] zeroinitializer
define void @_start() {{
    %ptr = alloca i64
    store atomic volatile i64 {}, i64* %ptr monotonic, align 1",
            self.mem_size,
            self.mem_size / 2
        );
        ir.push_str(&prelude);
        self.gen_ir_nodes(&mut ir, &mut ir_state, &self.ast.nodes);
        let epilogue = format!(
            r#"
    call i64 asm sideeffect "syscall", "=r,{{rax}},{{rdi}}"(i64 60, i64 0)
    ret void
}}"#
        );
        ir.push_str(&epilogue);
        ir
    }

    fn gen_ir_nodes(&self, ir: &mut String, state: &mut IrState, nodes: &VecDeque<Node>) {
        for node in nodes {
            match node {
                &Node::IncPtr(v, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let r = format!(
                        r"
    {ptr} = load atomic i64, i64* %ptr monotonic, align 1 ; Increment Pointer
    {mem_ptr} = add i64 {ptr}, {value}
    store atomic i64 {mem_ptr}, i64* %ptr monotonic, align 1",
                        ptr = i0,
                        mem_ptr = i1,
                        value = v
                    );
                    ir.push_str(&r);
                }
                &Node::DecPtr(v, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let r = format!(
                        r"
    {ptr} = load atomic i64, i64* %ptr monotonic, align 1 ; Decrement Pointer
    {mem_ptr} = sub i64 {ptr}, {value}
    store atomic i64 {mem_ptr}, i64* %ptr monotonic, align 1",
                        ptr = i0,
                        mem_ptr = i1,
                        value = v
                    );
                    ir.push_str(&r);
                }
                &Node::Increment(v, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let r = format!(
                        r"
    {ptr} = load atomic i64, i64* %ptr monotonic, align 1 ; Increment
    {mem_ptr} = getelementptr [{mem_size} x i8], [{mem_size} x i8]* @mem, i64 0, i64 {ptr}
    {mem_val} = load atomic volatile i8, i8* {mem_ptr} monotonic, align 1
    {new_mem_val} = add i8 {mem_val}, {value}
    store atomic volatile i8 {new_mem_val}, i8* {mem_ptr} monotonic, align 1",
                        mem_size = self.mem_size,
                        ptr = i0,
                        mem_ptr = i1,
                        mem_val = i2,
                        new_mem_val = i3,
                        value = v
                    );
                    ir.push_str(&r);
                }
                &Node::Decrement(v, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let r = format!(
                        r"
    {ptr} = load atomic i64, i64* %ptr monotonic, align 1 ; Decrement
    {mem_ptr} = getelementptr [{mem_size} x i8], [{mem_size} x i8]* @mem, i64 0, i64 {ptr}
    {mem_val} = load atomic volatile i8, i8* {mem_ptr} monotonic, align 1
    {new_mem_val} = sub i8 {mem_val}, {value}
    store atomic volatile i8 {new_mem_val}, i8* {mem_ptr} monotonic, align 1",
                        mem_size = self.mem_size,
                        ptr = i0,
                        mem_ptr = i1,
                        mem_val = i2,
                        new_mem_val = i3,
                        value = v
                    );
                    ir.push_str(&r);
                }
                &Node::Output(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let r = format!(
                        r#"
    {ptr} = load atomic i64, i64* %ptr monotonic, align 1 ; Output
    {mem_ptr} = getelementptr [{mem_size} x i8], [{mem_size} x i8]* @mem, i64 0, i64 {ptr}
    call i64 asm sideeffect "syscall", "=r,{{rax}},{{rdi}},{{rsi}},{{rdx}}"(i64 1, i64 1, i8* {mem_ptr}, i64 1)"#,
                        mem_size = self.mem_size,
                        ptr = i0,
                        mem_ptr = i1,
                    );
                    ir.push_str(&r);
                }
                &Node::Input(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let r = format!(
                        r#"
    {ptr} = load atomic i64, i64* %ptr monotonic, align 1 ; Input
    {mem_ptr} = getelementptr [{mem_size} x i8], [{mem_size} x i8]* @mem, i64 0, i64 {ptr}
    call i64 asm sideeffect "syscall", "=r,{{rax}},{{rdi}},{{rsi}},{{rdx}}"(i64 0, i64 0, i8* {mem_ptr}, i64 1)"#,
                        mem_size = self.mem_size,
                        ptr = i0,
                        mem_ptr = i1,
                    );
                    ir.push_str(&r);
                }
                &Node::Loop(ref nodes, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let header = state.label();
                    let body = state.label();
                    let end = state.label();
                    let r = format!(
                        r"
    br label %{header} ; Loop
{header}:
    {ptr} = load atomic i64, i64* %ptr monotonic, align 1
    {mem_ptr} = getelementptr [{mem_size} x i8], [{mem_size} x i8]* @mem, i64 0, i64 {ptr}
    {mem_val} = load atomic volatile i8, i8* {mem_ptr} monotonic, align 1
    {comp} = icmp eq i8 0, {mem_val}
    br i1 {comp}, label %{end}, label %{body}
{body}:",
                        ptr = i0,
                        mem_size = self.mem_size,
                        mem_ptr = i1,
                        mem_val = i2,
                        comp = i3,
                        header = header,
                        body = body,
                        end = end
                    );
                    ir.push_str(&r);
                    self.gen_ir_nodes(ir, state, nodes);
                    let r = format!(
                        r"
    br label %{header}
{end}:",
                        header = header,
                        end = end
                    );
                    ir.push_str(&r);
                }
            }
        }
    }
}
