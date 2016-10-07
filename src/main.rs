use std::fs::File;
use std::path::Path;
use std::io::{BufReader, Write};
use std::borrow::{Borrow, Cow};
use std::process::{Command, Stdio};

extern crate clap;
use clap::{App, Arg};

mod brainfuck;
use brainfuck::{Brainfuck, ParseError, ExecError};

fn main() {
    let matches = App::new("Brainfuck")
        .version("0.0.1")
        .author("Nick Massey <nickmass@nickmass.com>")
        .about("Parses brainfuck and interprets or compiles it")
        .arg(Arg::with_name("emit-ir")
             .short("S")
             .long("emit-ir")
             .help("Outputs llvm-ir to stdout"))
        .arg(Arg::with_name("compile")
             .short("c")
             .long("compile")
             .help("Compile to binary with llvm and gcc"))
        .arg(Arg::with_name("INPUT")
             .help("Sets the brainfuck file to parse")
             .required(true)
             .index(1))
        .get_matches();

    let compile_ir = matches.is_present("compile");
    let gen_ir = matches.is_present("emit-ir") || compile_ir;
    let input_file = matches.value_of("INPUT").unwrap();

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
        let ir = bf.gen_ir();
        if compile_ir {
            let mut llc = Command::new("llc")
                .arg("-O3")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("failed to execute llc");

            let _ = llc.stdin.as_mut().unwrap().write_fmt(format_args!("{}", ir));

            let llc_output = llc
                .wait_with_output()
                .expect("failed to get llc output");

            if !llc_output.status.success() {
                println!("failed to execute llc");
                return;
            }
            let file_name: &str = file_name.borrow();
            let output_name = Path::new(file_name).file_stem().unwrap();
            let mut gcc = Command::new("gcc")
                .arg("-O3")
                .arg("-x")
                .arg("assembler")
                .arg("-o")
                .arg(format!("{}", output_name.to_string_lossy()))
                .arg("-")
                .stdin(Stdio::piped())
                .spawn()
                .expect("failed to execute gcc");

            let _ = gcc.stdin.as_mut().unwrap().write_all(&llc_output.stdout);

            let r = gcc.wait().expect("failed to execute gcc");

            if !r.success() {
                println!("failed to execute gcc");
                return;
            }
        } else {
            println!("{}", ir);
        }
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
