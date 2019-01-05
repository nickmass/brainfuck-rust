use std::borrow::{Borrow, Cow};
use std::fs::File;
use std::io::{copy, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

extern crate clap;
use clap::{App, Arg};

mod brainfuck;
use brainfuck::{Brainfuck, ExecError, ParseError};

fn main() {
    let matches = App::new("Brainfuck")
        .version("0.0.1")
        .author("Nick Massey <nickmass@nickmass.com>")
        .about("Parses brainfuck and interprets or compiles it")
        .arg(
            Arg::with_name("emit-ir")
                .short("S")
                .long("emit-ir")
                .help("Outputs llvm-ir to stdout"),
        ).arg(
            Arg::with_name("compile")
                .short("c")
                .long("compile")
                .help("Compile to binary with llvm and clang"),
        ).arg(
            Arg::with_name("INPUT")
                .help("Sets the brainfuck file to parse")
                .required(true)
                .index(1),
        ).get_matches();

    let compile_ir = matches.is_present("compile");
    let gen_ir = matches.is_present("emit-ir") || compile_ir;
    let input_file = matches.value_of("INPUT").unwrap();

    let input_path = Path::new(&input_file);
    let file_name = input_path
        .file_name()
        .expect("No source file specified")
        .to_string_lossy();
    let directory = input_path
        .parent()
        .map(|x| x.to_string_lossy())
        .unwrap_or(Cow::Borrowed(""));
    let source = File::open(input_path).expect("Could not open source file.");
    let reader = BufReader::new(source);

    let bf = Brainfuck::parse(reader, file_name.borrow(), directory.borrow());

    let bf = match bf {
        Ok(bf) => bf,
        Err(ParseError::UnmatchedLoop(d)) => {
            println!(
                "error: unmatached loop --> {}:{}:{}",
                d.file, d.line, d.column
            );
            return;
        }
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

            let _ = llc.stdin.as_mut().unwrap().write_all(ir.as_bytes());

            let llc_output = llc.wait_with_output().expect("failed to get llc output");

            if !llc_output.status.success() {
                println!("failed to execute llc");
                return;
            }
            let file_name: &str = file_name.borrow();
            let output_name = Path::new(file_name).file_stem().unwrap();
            let mut clang = Command::new("clang")
                .arg("-O3")
                .arg("-x")
                .arg("assembler")
                .arg("-s")
                .arg("--static")
                .arg("-nostdlib")
                .arg("-Wl,--gc-sections")
                .arg("-Wl,-z,norelro")
                .arg("-Wl,--hash-style=gnu")
                .arg("-Wl,--build-id=none")
                .arg("-o")
                .arg(format!("{}", output_name.to_string_lossy()))
                .arg("-")
                .stdin(Stdio::piped())
                .spawn()
                .expect("failed to execute clang");

            copy(
                &mut llc_output.stdout.as_slice(),
                &mut clang.stdin.as_mut().unwrap(),
            ).unwrap();

            let r = clang.wait().expect("failed to execute clang");

            if !r.success() {
                println!("failed to execute clang");
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
            println!(
                "exception: out of bounds access --> {}:{}:{}",
                d.file, d.line, d.column
            );
            return;
        }
    }
}
