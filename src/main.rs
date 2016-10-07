extern crate libc;

use std::env;
use std::fs::File;
use std::path::Path;
use std::io::BufReader;
use std::io::Read;
use std::borrow::{Borrow, Cow};
use std::collections::VecDeque;

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
struct DebugInfo<'a> {
    directory: &'a str,
    file: &'a str,
    line: u32,
    column: u32,
}

fn main() {
    let mut args = env::args();
    let _executable = args.next();
    let input_file = args.next().expect("No source file specified");
    let input_path = Path::new(&input_file);
    let file_name = input_path.file_name().expect("No source file specified").to_string_lossy();
    let directory = input_path.parent().map(|x| x.to_string_lossy()).unwrap_or(Cow::Borrowed(""));
    let source = File::open(input_path).expect("Could not open source file.");
    let reader = BufReader::new(source);

    let mut symbols = VecDeque::new();
    let mut line = 1;
    let mut column = 1;
    for byte in reader.bytes() {
        let debug = DebugInfo {
            directory: directory.borrow(),
            file: file_name.borrow(),
            line: line,
            column: column,
        };
        if byte.is_err() { break; }
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
            },
            _ => (),
        }
        column += 1;
    }

    let ast = Ast::parse(symbols);

    match ast {
        Ok(mut ast) => {
            ast.optimize();
            let prog = Program::new(ast, 100000);
/*
            match prog.exec() {
                Err(ExecError::OutOfBounds(d)) => {
                    println!("exception: out of bounds memory access --> {}:{}:{}", d.file, d.line, d.column);
                },
                _ => ()
            }
             */
            let ir = prog.gen_ir();
            println!("{}", ir);
        },
        Err(ParseError::UnmatchedLoop(d)) => {
            println!("error: unmatched loop --> {}:{}:{}", d.file, d.line, d.column);
        }
    }
}

#[derive(Debug)]
enum ParseError<'a> {
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
    nodes: VecDeque<Node<'a>>
}

impl<'a> Ast<'a> {
    pub fn parse(mut symbols: VecDeque<Symbol<'a>>) -> Result<Ast<'a>, ParseError<'a>> {
        let mut ast = Ast {
            nodes: VecDeque::new()
        };

        while {
            match Self::add_node(&mut ast.nodes, &mut symbols) {
                ParseResult::Ok => true,
                ParseResult::UnmatchedLoop(d) => return Err(ParseError::UnmatchedLoop(d)),
                ParseResult::CloseLoop(d) => return Err(ParseError::UnmatchedLoop(d)),
                ParseResult::Eof => false,
            }
        }{}

        Ok(ast)
    }

    fn add_node(nodes: &mut VecDeque<Node<'a>>, symbols: &mut VecDeque<Symbol<'a>>)
                -> ParseResult<'a> {
        let symbol = symbols.pop_front();

        match symbol {
            Some(Symbol::OpenBlock(d)) => {
                let mut loop_body = VecDeque::new();
                while {
                    match Self::add_node(&mut loop_body, symbols) {
                        ParseResult::UnmatchedLoop(d) => {
                            return ParseResult::UnmatchedLoop(d);
                        },
                        ParseResult::CloseLoop(_) => false,
                        ParseResult::Ok => true,
                        ParseResult::Eof => {
                            return ParseResult::UnmatchedLoop(d);
                        }
                    }
                }{}
                nodes.push_back(Node::Loop(loop_body, d));
                ParseResult::Ok
            },
            Some(Symbol::CloseBlock(d)) => {
                ParseResult::CloseLoop(d)
            },
            Some(Symbol::IncPtr(d)) => {
                nodes.push_back(Node::IncPtr(1, d));
                ParseResult::Ok
            },
            Some(Symbol::DecPtr(d)) => {
                nodes.push_back(Node::DecPtr(1, d));
                ParseResult::Ok
            },
            Some(Symbol::Increment(d)) => {
                nodes.push_back(Node::Increment(1, d));
                ParseResult::Ok
            },
            Some(Symbol::Decrement(d)) => {
                nodes.push_back(Node::Decrement(1, d));
                ParseResult::Ok
            },
            Some(Symbol::Output(d)) => {
                nodes.push_back(Node::Output(d));
                ParseResult::Ok
            },
            Some(Symbol::Input(d)) => {
                nodes.push_back(Node::Input(d));
                ParseResult::Ok
            },
            None => {
                ParseResult::Eof
            }
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
                },
                Node::IncPtr(v, d) => {
                    let mut value = v;
                    while let Some(&Node::IncPtr(v, _)) = nodes.front() {
                        value = value.wrapping_add(v);
                        nodes.pop_front();
                    }
                    opt_nodes.push_back(Node::IncPtr(value, d));
                },
                Node::DecPtr(v, d) => {
                    let mut value = v;
                    while let Some(&Node::DecPtr(v, _)) = nodes.front() {
                        value = value.wrapping_add(v);
                        nodes.pop_front();
                    }
                    opt_nodes.push_back(Node::DecPtr(value, d));
                },
                Node::Increment(v, d) => {
                    let mut value = v;
                    while let Some(&Node::Increment(v, _)) = nodes.front() {
                        value = value.wrapping_add(v);
                        nodes.pop_front();
                    }
                    opt_nodes.push_back(Node::Increment(value, d));
                },
                Node::Decrement(v, d) => {
                    let mut value = v;
                    while let Some(&Node::Decrement(v, _)) = nodes.front() {
                        value = value.wrapping_add(v);
                        nodes.pop_front();
                    }
                    opt_nodes.push_back(Node::Decrement(value, d));
                },
                Node::Input(d) => {
                    opt_nodes.push_back(Node::Input(d));
                },
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
    pub fn new(mem_size: usize) -> ProgramState {
        let mut mem = Vec::with_capacity(mem_size);
        for _ in 0..mem_size { mem.push(0); }

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
    mem_size: usize,
}

impl IrState {
    pub fn new(mem_size: usize) -> IrState {
        IrState {
            next_label: 0,
            mem_size: mem_size,
        }
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

enum ExecError<'a> {
    OutOfBounds(&'a DebugInfo<'a>)
}

impl<'a> Program<'a> {
    pub fn new(ast: Ast<'a>, mem_size: usize) -> Program<'a> {
        let prog = Program {
            ast: ast,
            mem_size: mem_size,
        };

        prog
    }

    pub fn exec(&'a self) -> Result<(), ExecError<'a>> {
        let mut state = ProgramState::new(self.mem_size);
        Self::exec_nodes(&mut state, &self.ast.nodes)
    }

    fn exec_nodes(state: &mut ProgramState, nodes: &'a VecDeque<Node<'a>>)
                  -> Result<(), ExecError<'a>> {
        for node in nodes {
            match node {
                &Node::IncPtr(v, _) => state.ptr = state.ptr.wrapping_add(v),
                &Node::DecPtr(v, _) => state.ptr = state.ptr.wrapping_sub(v),
                &Node::Increment(v, ref d) => {
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d))
                    }
                    let val = state.mem[state.ptr].wrapping_add(v);
                    state.mem[state.ptr] = val;
                },
                &Node::Decrement(v, ref d) => {
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d))
                    }
                    let val = state.mem[state.ptr].wrapping_sub(v);
                    state.mem[state.ptr] = val;
                },
                &Node::Output(ref d) => {
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d))
                    }
                    print!("{}", state.mem[state.ptr] as char)
                },
                &Node::Input(ref d) => {
                    let val = unsafe { libc::getchar() };
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d))
                    }
                    state.mem[state.ptr] = val as u8;
                },
                &Node::Loop(ref nodes, ref d) => {
                    if state.is_oob() {
                        return Err(ExecError::OutOfBounds(d))
                    }
                    while state.mem[state.ptr] != 0 {
                        match Self::exec_nodes(state, nodes) {
                            Err(e) => return Err(e),
                            _ => (),
                        }
                        if state.is_oob() {
                            return Err(ExecError::OutOfBounds(d))
                        }
                    }
                },
            }
        }

        Ok(())
    }

    pub fn gen_ir(&self) -> String {
        let mut ir = String::new();
        let mut ir_state = IrState::new(self.mem_size);
        let prelude = format!("
declare i32 @getchar()
declare i32 @putchar(i32)
define i32 @main() {{
\t%mem = alloca i8, i32 {}
\t%ptr = alloca i32
\tstore i32 {}, i32* %ptr", self.mem_size, self.mem_size / 2);
        ir.push_str(&prelude);
        Self::gen_ir_nodes(&mut ir, &mut ir_state, &self.ast.nodes);
        let epilogue = format!("
\tret i32 0
}}");
        ir.push_str(&epilogue);
        ir
    }

    pub fn gen_ir_nodes(ir: &mut String, state: &mut IrState, nodes: &VecDeque<Node>) {
        for node in nodes {
            match node {
                &Node::IncPtr(v, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let r = format!("
\t{ptr} = load i32* %ptr
\t{mem_ptr} = add i32 {ptr}, {value}
\tstore i32 {mem_ptr}, i32* %ptr",
                                    ptr=i0, mem_ptr=i1, value=v);
                    ir.push_str(&r);
                },
                &Node::DecPtr(v, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let r = format!("
\t{ptr} = load i32* %ptr
\t{mem_ptr} = sub i32 {ptr}, {value}
\tstore i32 {mem_ptr}, i32* %ptr",
                                    ptr=i0, mem_ptr=i1, value=v);
                    ir.push_str(&r);
                },
                &Node::Increment(v, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let r = format!("
\t{ptr} = load i32* %ptr;
\t{mem_ptr} = getelementptr i8* %mem, i32 {ptr}
\t{mem_val} = load i8* {mem_ptr}
\t{new_mem_val} = add i8 {mem_val}, {value}
\tstore i8 {new_mem_val}, i8* {mem_ptr}",
                                    ptr=i0,  mem_ptr=i1, mem_val=i2, new_mem_val=i3, value=v);
                    ir.push_str(&r);
                },
                &Node::Decrement(v, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let r = format!("
\t{ptr} = load i32* %ptr;
\t{mem_ptr} = getelementptr i8* %mem, i32 {ptr}
\t{mem_val} = load i8* {mem_ptr}
\t{new_mem_val} = sub i8 {mem_val}, {value}
\tstore i8 {new_mem_val}, i8* {mem_ptr}",
                    ptr=i0,  mem_ptr=i1, mem_val=i2, new_mem_val=i3, value=v);
                    ir.push_str(&r);
                },
                &Node::Output(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let r = format!("
\t{ptr} = load i32* %ptr;
\t{mem_ptr} = getelementptr i8* %mem, i32 {ptr}
\t{mem_val} = load i8* {mem_ptr}
\t{mem_val_32} = sext i8 {mem_val} to i32
\tcall i32 @putchar(i32 {mem_val_32})",
                                    ptr=i0,  mem_ptr=i1, mem_val=i2, mem_val_32=i3);
                    ir.push_str(&r);
                },
                &Node::Input(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let r = format!("
\t{ptr} = load i32* %ptr;
\t{mem_ptr} = getelementptr i8* %mem, i32 {ptr}
\t{input} = call i32 @getchar()
\t{input_8} = trunc i32 {input} to i8
\tstore i8 {input_8}, i8* {mem_ptr}",
                                    ptr=i0,  mem_ptr=i1, input=i2, input_8=i3);
                    ir.push_str(&r);
                },
                &Node::Loop(ref nodes, _) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let header =state.label();
                    let body = state.label();
                    let end = state.label();
                    let r = format!("
\tbr label %{header}
{header}:
\t{ptr} = load i32* %ptr;
\t{mem_ptr} = getelementptr i8* %mem, i32 {ptr}
\t{mem_val} = load i8* {mem_ptr}
\t{comp} = icmp eq i8 0, {mem_val}
\tbr i1 {comp}, label %{end}, label %{body}
{body}:", ptr=i0, mem_ptr=i1, mem_val=i2, comp=i3, header=header, body=body, end=end);
                    ir.push_str(&r);
                    Self::gen_ir_nodes(ir, state, nodes);
                    let r = format!("
\tbr label %{header}
{end}:", header=header, end=end);
                    ir.push_str(&r);

                },
            }
        }
    }
}
