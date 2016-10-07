use std::env;
use std::fs::File;
use std::path::Path;
use std::io::BufReader;
use std::borrow::{Borrow, Cow};

mod brainfuck;
use brainfuck::{Brainfuck, ParseError, ExecError};

fn main() {
    let mut args = env::args();
    let _executable = args.next();
    let gen_ir = true;
    let input_file = args.next().expect("No source file specified");
    let input_path = Path::new(&input_file);
    let file_name = input_path.file_name().expect("No source file specified").to_string_lossy();
    let directory = input_path.parent().map(|x| x.to_string_lossy()).unwrap_or(Cow::Borrowed(""));
    let source = File::open(input_path).expect("Could not open source file.");
    let reader = BufReader::new(source);

    let bf = Brainfuck::parse(reader, file_name.borrow(), directory.borrow());

    let bf = match bf {
        Ok(bf) => bf,
        Err(ParseError::UnmatchedLoop(d)) => {
            println!("error: unmatached loop --> {}:{}:{}", d.file, d.line, d.column);
            return
        },
    };

    if gen_ir {
        println!("{}", bf.gen_ir());
        return;
    }

    match bf.exec() {
        Ok(_) => (),
        Err(ExecError::OutOfBounds(d)) => {
            println!("exception: out of bounds access --> {}:{}:{}", d.file, d.line, d.column);
            return
        },
    }
}
