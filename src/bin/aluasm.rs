// AluVM Assembler
// To find more on AluVM please check <https://www.aluvm.org>
//
// Designed & written in 2021 by
//     Dr. Maxim Orlovsky <orlovsky@pandoracore.com>
// for Pandora Core AG

use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::exit;

use aluasm::ast::Program;
use aluasm::parser::{Parser, Rule};
use aluasm::{AccessError, LexerError, MainError};
use aluvm::isa::ReservedOp;
use aluvm::libs::Lib;
use clap::{AppSettings, Clap};
use pest::Parser as ParserTrait;

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Clap)]
#[clap(
    name = "aluasm",
    bin_name = "aluasm",
    author,
    version,
    about,
    setting = AppSettings::ColoredHelp
)]
pub struct Args {
    /// Set verbosity level
    ///
    /// Can be used multiple times to increase verbosity
    #[clap(short, long, global = true, parse(from_occurrences))]
    pub verbose: u8,

    /// Dumps debug raw compilation log to the destination location
    #[clap(long, global = true)]
    pub dump: Option<PathBuf>,

    /// Tests creation of library from the generated object module
    #[clap(long, global = true)]
    pub test_lib: bool,

    /// Tests disassembly of the generated library
    #[clap(long, global = true)]
    pub test_disassemble: bool,

    /// Directory to output object files into
    #[clap(short, long, global = true, default_value = "./build/objects")]
    pub output: PathBuf,

    /// List of source files to compile
    pub files: Vec<PathBuf>,
}

fn main() {
    let args = Args::parse();
    match compile(args) {
        Ok(_) => exit(0),
        Err(err) => {
            eprintln!("{}", err);
            exit(1)
        }
    }
}

fn compile(args: Args) -> Result<(), MainError> {
    let dir = args.output.clone();
    fs::create_dir_all(dir.clone()).map_err(|err| AccessError::OutputDir {
        dir: dir.to_string_lossy().to_string(),
        details: Box::new(err),
    })?;

    for file in &args.files {
        compile_file(file, &args)?;
    }

    Ok(())
}

fn compile_file(file: &PathBuf, args: &Args) -> Result<(), MainError> {
    let file_name =
        file.file_name().unwrap_or(OsStr::new("<noname>")).to_string_lossy().to_string();

    let mut dump = args
        .dump
        .as_ref()
        .map(|dump_file| {
            if args.verbose >= 1 {
                eprintln!(
                    "\x1B[1;33mWill be dumping detailed compilation log into `{}`\x1B[0m",
                    dump_file.display()
                )
            }
            let dump_file_name = dump_file.to_string_lossy().to_string();
            File::create(dump_file).map_err(|err| AccessError::DumpFileCreation {
                file: dump_file_name,
                details: Box::new(err),
            })
        })
        .transpose()?;

    eprintln!(
        "\x1B[1;32mCompiling\x1B[0m {} ({})",
        file_name,
        file.canonicalize().unwrap_or_default().display()
    );

    let mut s = String::new();
    let mut fd = File::open(file).map_err(|err| AccessError::FileNotFound {
        file: file_name.clone(),
        details: Box::new(err),
    })?;
    fd.read_to_string(&mut s).map_err(|err| AccessError::FileNoAccess {
        file: file_name.clone(),
        details: Box::new(err),
    })?;

    let pairs = Parser::parse(Rule::program, &s)
        .map_err(|err| MainError::Parser(file_name.clone(), err))?;
    let (program, issues) =
        Program::analyze(pairs.into_iter().next().ok_or(LexerError::ProgramAbsent)?)?;

    if issues.has_errors() {
        return Err(MainError::Syntax(
            file_name,
            issues.count_errors(),
            issues.count_warnings(),
            issues.to_string(),
        ));
    }
    eprintln!("{}", issues);

    let (module, issues) = program.compile(&mut dump)?;
    if issues.has_errors() {
        return Err(MainError::Compile(
            file_name,
            issues.count_errors(),
            issues.count_warnings(),
            issues.to_string(),
        ));
    }
    eprintln!("{}", issues);

    let mut dest = args.output.clone();
    dest.push(file.file_name().unwrap_or_default());
    dest.set_extension("ao");
    let dest_name = dest.to_string_lossy().to_string();
    let mut fd = File::create(dest).map_err(|err| AccessError::ObjFileCreation {
        file: dest_name.clone(),
        details: Box::new(err),
    })?;
    module.write(&mut fd).map_err(|err| AccessError::ObjFileWrite {
        file: dest_name.clone(),
        details: Box::new(err),
    })?;

    if args.verbose >= 2 {
        eprintln!("\x1B[1;33m Printing\x1B[0m module dump:");
        println!("{:?}", module);
    }

    if args.test_lib || args.test_disassemble {
        let lib: Lib<ReservedOp> =
            Lib::with(&module.isae, module.code, module.data, module.libs).map_err(|err| {
                AccessError::LibraryCreation { file: dest_name.clone(), details: err }
            })?;

        if args.verbose >= 2 {
            eprintln!("\x1B[1;33m Printing\x1B[0m library dump:");
            println!("{}", lib);
        }

        if args.test_disassemble {
            let code =
                lib.disassemble().map_err(|_| AccessError::Disassembling { file: dest_name })?;

            if args.verbose >= 2 {
                eprintln!("\x1B[1;33m Printing\x1B[0m module disassemply:");
                for instr in code {
                    println!("\t\t{}", instr);
                }
            }
        }
    }

    Ok(())
}
