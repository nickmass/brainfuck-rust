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
            let mut prog = Program::new(ast, 4096);
            prog.exec();
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
    state: ProgramState,
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

impl<'a> Program<'a> {
    pub fn new(ast: Ast<'a>, mem_size: usize) -> Program<'a> {
        let prog = Program {
            ast: ast,
            mem_size: mem_size,
            state: ProgramState::new(mem_size),
        };

        prog
    }

    fn reset(&mut self) {
        self.state = ProgramState::new(self.mem_size);
    }

    pub fn exec(&mut self) {
        self.reset();

        Self::exec_nodes(&mut self.state, &self.ast.nodes);
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
                &Node::Input(_) => (),
                &Node::Loop(_,ref nodes) => {
                    while state.read() != 0 {
                        Self::exec_nodes(state, nodes);
                    }
                },
            }
        }
    }
}
