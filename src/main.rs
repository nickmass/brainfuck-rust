use std::borrow::{Borrow, Cow};
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;
use std::process::Command;

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
                .help("Compile to binary with llvm"),
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
            let file_name: &str = file_name.borrow();
            let output_name = Path::new(file_name).file_stem().unwrap().to_string_lossy();
            let ir_file_name = format!("{}.ll", output_name);
            let bc_file_name = format!("{}.bc", output_name);
            let o_file_name = format!("{}.o", output_name);

            let mut ir_file = File::create(&ir_file_name).unwrap();
            let _ = ir_file.write_all(ir.as_bytes()).unwrap();

            let opt = Command::new("opt")
                .arg("-O3")
                .arg("-strip-debug")
                .arg(&ir_file_name)
                .arg("-o")
                .arg(&bc_file_name)
                .status()
                .expect("failed to execute opt");

            if !opt.success() {
                println!("failed to execute opt");
                return;
            }

            let llc = Command::new("llc")
                .arg("-O3")
                .arg("-filetype=obj")
                .arg(&bc_file_name)
                .arg("-o")
                .arg(&o_file_name)
                .status()
                .expect("failed to execute llc");

            if !llc.success() {
                println!("failed to execute llc");
                return;
            }

            let lld = Command::new("ld.lld")
                .arg("-static")
                .arg("-nostdlib")
                .arg("--gc-sections")
                .arg("-s")
                .arg("-z")
                .arg("norelro")
                .arg("--hash-style=gnu")
                .arg("--build-id=none")
                .arg(&o_file_name)
                .arg("-o")
                .arg(output_name.to_string())
                .status()
                .expect("failed to execute lld");

            if !lld.success() {
                println!("failed to execute lld");
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
