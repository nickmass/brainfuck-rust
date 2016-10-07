extern crate libc;

use std::env;
use std::fs::File;
use std::path::Path;
use std::io::BufReader;
use std::io::Read;
use std::borrow::Borrow;
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
    let source = File::open(input_path).expect("Could not open source file.");
    let reader = BufReader::new(source);

    let mut symbols = VecDeque::new();
    let mut line = 0;
    let mut column = 0;
    for byte in reader.bytes() {
        let debug = DebugInfo {
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
                continue;
            },
            _ => (),
        }
        column += 1;
    }

    let ast = Ast::parse(symbols);

    match ast {
        Ok(ast) => {
            let prog = Program::new(ast, 4096);
            //prog.exec();
            let ir = prog.gen_ir();
            println!("{}", ir);
        },
        Err(ParseError::UnmatchedLoop(d)) => {
            println!("Unmatched Loop: {} {}:{}", d.file, d.line, d.column);
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
    Loop(DebugInfo<'a>, Vec<Node<'a>>),
    IncPtr(DebugInfo<'a>),
    DecPtr(DebugInfo<'a>),
    Increment(DebugInfo<'a>),
    Decrement(DebugInfo<'a>),
    Output(DebugInfo<'a>),
    Input(DebugInfo<'a>),
}

#[derive(Debug)]
struct Ast<'a> {
    nodes: Vec<Node<'a>>
}

impl<'a> Ast<'a> {
    pub fn parse(mut symbols: VecDeque<Symbol<'a>>) -> Result<Ast<'a>, ParseError<'a>> {
        let mut ast = Ast {
            nodes: Vec::new()
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

    fn add_node(nodes: &mut Vec<Node<'a>>, symbols: &mut VecDeque<Symbol<'a>>)
                -> ParseResult<'a> {
        let symbol = symbols.pop_front();

        match symbol {
            Some(Symbol::OpenBlock(d)) => {
                let mut loop_body = Vec::new();
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
                nodes.push(Node::Loop(d, loop_body));
                ParseResult::Ok
            },
            Some(Symbol::CloseBlock(d)) => {
                ParseResult::CloseLoop(d)
            },
            Some(Symbol::IncPtr(d)) => {
                nodes.push(Node::IncPtr(d));
                ParseResult::Ok
            },
            Some(Symbol::DecPtr(d)) => {
                nodes.push(Node::DecPtr(d));
                ParseResult::Ok
            },
            Some(Symbol::Increment(d)) => {
                nodes.push(Node::Increment(d));
                ParseResult::Ok
            },
            Some(Symbol::Decrement(d)) => {
                nodes.push(Node::Decrement(d));
                ParseResult::Ok
            },
            Some(Symbol::Output(d)) => {
                nodes.push(Node::Output(d));
                ParseResult::Ok
            },
            Some(Symbol::Input(d)) => {
                nodes.push(Node::Input(d));
                ParseResult::Ok
            },
            None => {
                ParseResult::Eof
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
            ptr: 0,
            mem: mem,
            mem_size: mem_size,
        }
    }

    fn read(&self) -> u8 {
        self.mem[self.ptr % self.mem_size]
    }

    fn write(&mut self, value: u8) {
        self.mem[self.ptr % self.mem_size] = value;
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

impl<'a> Program<'a> {
    pub fn new(ast: Ast<'a>, mem_size: usize) -> Program<'a> {
        let prog = Program {
            ast: ast,
            mem_size: mem_size,
        };

        prog
    }

    pub fn exec(&self) {
        let mut state = ProgramState::new(self.mem_size);
        Self::exec_nodes(&mut state, &self.ast.nodes);
    }

    fn exec_nodes(state: &mut ProgramState, nodes: &Vec<Node>) {
        for node in nodes {
            match node {
                &Node::IncPtr(_) => state.ptr = state.ptr.wrapping_add(1),
                &Node::DecPtr(_) => state.ptr = state.ptr.wrapping_sub(1),
                &Node::Increment(_) => {
                    let val = state.read().wrapping_add(1);
                    state.mem[state.ptr % state.mem_size] = val;
                },
                &Node::Decrement(_) => {
                    let val = state.read().wrapping_sub(1);
                    state.mem[state.ptr % state.mem_size] = val;
                },
                &Node::Output(_) => print!("{}", state.read() as char),
                &Node::Input(_) => {
                    let val = unsafe { libc::getchar() };
                    state.mem[state.ptr % state.mem_size] = val as u8;
                },
                &Node::Loop(_,ref nodes) => {
                    while state.read() != 0 {
                        Self::exec_nodes(state, nodes);
                    }
                },
            }
        }
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
\tstore i32 0, i32* %ptr", self.mem_size);
        ir.push_str(&prelude);
        Self::gen_ir_nodes(&mut ir, &mut ir_state, &self.ast.nodes);
        let epilogue = format!("
\tret i32 0
}}");
        ir.push_str(&epilogue);
        ir
    }

    pub fn gen_ir_nodes(ir: &mut String, state: &mut IrState, nodes: &Vec<Node>) {
        for node in nodes {
            match node {
                &Node::IncPtr(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let r = format!("
\t{0} = load i32* %ptr
\t{1} = add i32 1, {0}
\tstore i32 {1}, i32* %ptr", i0, i1);
                    ir.push_str(&r);
                },
                &Node::DecPtr(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let r = format!("
\t{0} = load i32* %ptr
\t{1} = sub i32 {0}, 1
\tstore i32 {1}, i32* %ptr", i0, i1);
                    ir.push_str(&r);
                },
                &Node::Increment(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let i4 = state.ident();
                    let r = format!("
\t{1} = load i32* %ptr;
\t{2} = urem i32 {1}, {0}
\t{3} = getelementptr i8* %mem, i32 {2}
\t{4} = load i8* {3}
\t{5} = add i8 {4}, 1
\tstore i8 {5}, i8* {3}", state.mem_size, i0, i1, i2, i3, i4);
                    ir.push_str(&r);
                },
                &Node::Decrement(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let i4 = state.ident();
                    let r = format!("
\t{1} = load i32* %ptr;
\t{2} = urem i32 {1}, {0}
\t{3} = getelementptr i8* %mem, i32 {2}
\t{4} = load i8* {3}
\t{5} = sub i8 {4}, 1
\tstore i8 {5}, i8* {3}", state.mem_size, i0, i1, i2, i3, i4);
                    ir.push_str(&r);
                },
                &Node::Output(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let i4 = state.ident();
                    let r = format!("
\t{1} = load i32* %ptr;
\t{2} = urem i32 {1}, {0}
\t{3} = getelementptr i8* %mem, i32 {2}
\t{4} = load i8* {3}
\t{5} = sext i8 {4} to i32
\tcall i32 @putchar(i32 {5})", state.mem_size, i0, i1, i2, i3, i4);
                    ir.push_str(&r);
                },
                &Node::Input(_) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let i4 = state.ident();
                    let r = format!("
\t{1} = load i32* %ptr;
\t{2} = urem i32 {1}, {0}
\t{3} = getelementptr i8* %mem, i32 {2}
\t{4} = call i32 @getchar()
\t{5} = trunc i32 {4} to i8
\tstore i8 {5}, i8* {3}", state.mem_size, i0, i1, i2, i3, i4);
                    ir.push_str(&r);
                },
                &Node::Loop(_,ref nodes) => {
                    let i0 = state.ident();
                    let i1 = state.ident();
                    let i2 = state.ident();
                    let i3 = state.ident();
                    let i4 = state.ident();
                    let header =state.label();
                    let body = state.label();
                    let end = state.label();
                    let r = format!("
\tbr label %{header}
{header}:
\t{1} = load i32* %ptr;
\t{2} = urem i32 {1}, {0}
\t{3} = getelementptr i8* %mem, i32 {2}
\t{4} = load i8* {3}
\t{5} = icmp eq i8 0, {4}
\tbr i1 {5}, label %{end}, label %{body}
{body}:", state.mem_size, i0, i1, i2, i3, i4, header=header, body=body, end=end);
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
