use std::fs;
use std::path::PathBuf;
use std::env;
use std::process::Command;
use std::string::FromUtf8Error;
use clap::{Parser, ValueEnum, builder::PossibleValue};
use tr::*;

#[derive(Debug, )]
enum ToUtf8Error {
    UTF8DecodeError(FromUtf8Error),
}

impl From<FromUtf8Error> for ToUtf8Error {
    fn from(e: FromUtf8Error) -> Self {
        Self::UTF8DecodeError(e)
    }
}

fn convert_to_utf8(bytes: &[u8]) -> Result<String, ToUtf8Error> {
    // TODO: more portable encoding
    match String::from_utf8(bytes.to_vec()) {
        Ok(o) => { Ok(o) },
        Err(e) => { Err(ToUtf8Error::from(e)) }
    }
}

fn get_cargo_directories(path_str: &str) -> Vec<PathBuf> {
    let mut dir_pathes = Vec::<PathBuf>::new();
    // save the last index of every directory's subitems in dir_pathes
    let mut dir_sizes = Vec::<usize>::new();

    let mut marked_pathes = Vec::<PathBuf>::new();

    let path = std::fs::canonicalize(&path_str)
        .expect(&tr!(
                "Failed make directory \"{0}\" canonicalized.",
                path_str));

    dir_pathes.push(path);
    dir_sizes.push(1);
 
    loop {
        let mut marked_cargo_dir = false;
        if dir_pathes.is_empty() { break; }

        let dir_path = dir_pathes
            .last()
            .expect(&tr!("at least one path buf"));

        let sub_items = fs::read_dir(dir_path)
            .expect(
                &tr!("read directory failed \"{0}\"", 
                    path_str))
            .map(|a| {
                a.expect(&tr!("read directory failed."))
                    .path()
            })
        .collect::<Vec<PathBuf>>();

        if sub_items.iter().any(|a| {
            a.as_path().file_name()
                .expect(&tr!("get file name failed"))
                .to_str()
                .expect(
                    &tr!("OsString to String failed.")) == "Cargo.toml"
        }) {
           marked_pathes.push(dir_pathes.last().unwrap().clone());
           marked_cargo_dir = true;
        }

        let sub_items = sub_items.iter()
            .filter(|a| {
                !a.as_path().file_name()
                    .expect(&tr!("get file name failed."))
                    .to_str()
                    .expect(
                        &tr!("OsString to String failed."))
                    .starts_with(".") 
                    && a.metadata()
                    .expect(&tr!("get metadata error"))
                    .is_dir()
            })
        .filter(|a| {
            let file_name = a.as_path().file_name()
                .expect(&tr!("get file name failed."))
                .to_str()
                .expect(&tr!("OsString to String failed."));
            if marked_cargo_dir {
                file_name != "target" 
                && file_name != "src" 
            } else {
                true
            }
        })
        .map(|a| {
            let mut p = dir_pathes.last().unwrap().clone();
            p.push(a);
            p
        })
        .collect::<Vec<PathBuf>>();

        *dir_sizes.last_mut().unwrap() -= 1;
        dir_sizes.push(sub_items.len() + dir_sizes.last().unwrap());

        if *dir_sizes.last().unwrap() == 0 {
            dir_sizes.pop();
        }
 
        dir_pathes.pop();
        dir_pathes.extend(sub_items);
    }

    return marked_pathes;
}

#[derive(PartialEq, Debug, Default, Clone, Copy, Eq, PartialOrd, Ord, )]
enum GeneratingType {
    #[default]
    BashCommands,
    RunAsSubprocess,
    DryRunDebug,
}

/*
if args.len() >= 3 {
    match args[2].as_ref() {
        "cmd" | "cmds" | "bash_cmds" | "bash_cmd" | "bash_commands" => GeneratingType::BashCommands,
        "dry_run" | "dr" => GeneratingType::DryRunDebug,
        "direct" | "subprocess" => GeneratingType::RunAsSubprocess,
        other => panic!("unknow generating type: {}", other)
    }
} else {
    GeneratingType::default()
};
*/

impl ValueEnum for GeneratingType {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::BashCommands, Self::RunAsSubprocess, Self::DryRunDebug]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Self::BashCommands => {
                PossibleValue::new("bash-commands")
                    .help(
                        &tr!("generating bash-like commands."))
                    .aliases(["cmd", "cmds", "bash_cmds", "bash_commands"])
            }
            Self::RunAsSubprocess => {
                PossibleValue::new("run-as-subprocess")
                    .help(
                        &tr!("directly run cargo as subprocess."))
                    .aliases(["direct", "subprocess", "directly"])
            }
            Self::DryRunDebug => {
                PossibleValue::new("dry-run-debug")
                    .help(&tr!("dry run and output actions"))
                    .aliases(["dry_run", "dry-run", "dr"])

            }
        })
    }
}


#[derive(Debug)]
struct ProcessExitError {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
enum ProcessDirError {
    IoError(std::io::Error),
    ProcessExitError(
        ProcessExitError)
}

impl From<std::io::Error> for ProcessDirError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

fn process_dir(cargo_dir: &PathBuf, ge_ty: GeneratingType) -> Result<(), ProcessDirError> {
    let old_dir = env::current_dir()?;

    let old_dir_str = old_dir.to_str()
        .expect(&tr!("PathBuf to &str error"));
    let dest_dir_str = cargo_dir.to_str()
        .expect(&tr!("PathBuf to &str error"));

    match ge_ty {
        GeneratingType::BashCommands => {
            println!("cd {}", dest_dir_str);
            println!("cargo clean");
            println!("cd {}", old_dir_str);
            Ok(())
        },
        GeneratingType::RunAsSubprocess => {
            env::set_current_dir(cargo_dir)?;

            let output = Command::new("cargo").arg("clean")
                .output()
                .expect(&tr!("Start `cargo clean` failed."));
            if !output.status.success() {
                env::set_current_dir(old_dir)?;
                return Err(ProcessDirError::ProcessExitError(ProcessExitError {
                    code: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                }))
            }
            env::set_current_dir(old_dir)?;
            Ok(())
        },
        GeneratingType::DryRunDebug => {
            eprintln!("RUN: cargo clean at {}", dest_dir_str);
            Ok(())
        }
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    // starting point of directories, would be "./" if it isn't supplied.
    target_dir: Option<String>,
    // generaty types: bash commands(default), output debug(dry run), direct run as subprocess.
    #[arg(short, long, value_enum, default_value_t)]
    generating_type: GeneratingType
}

fn main() {
    tr::tr_init!("./i18n");

    let _test_i18n = false;
    if _test_i18n {
        let f = std::fs::File::open("i18n/mo/zh/rust_recursive_clean.mo")
            .expect("can't load i18n info");

        let catalog = gettext::Catalog::parse(f).expect("could not parse the catalog");
        set_translator!(catalog);
    }


    let cli = Cli::parse();
    let path_str = cli.target_dir.or(Some("./".to_owned())).unwrap();

    let ge_ty = cli.generating_type;

    let mut failed_list = Vec::new();

    let marked_pathes = get_cargo_directories(&path_str);

    if ge_ty == GeneratingType::BashCommands 
        || ge_ty == GeneratingType::DryRunDebug {

        println!("{}",
            tr!("# root path: {}", 
                format!("{:?}", fs::canonicalize(path_str).unwrap()))
            );
    }

    marked_pathes
        .iter()
        .for_each(|a| {
            match process_dir(a, ge_ty.clone()) {
                Ok(_) => {},
                Err(pde) => {
                    match pde {
                        ProcessDirError::IoError(ie) => {
                            panic!("{:?}", ie);
                        },
                        ProcessDirError::ProcessExitError(pee) => {
                            failed_list.push(pee);
                        }
                    }
                }
            }
    });

    match ge_ty {
        GeneratingType::RunAsSubprocess => {
            
            failed_list.iter()
                .for_each(|a| {
                    
                    println!("{{");
                    println!("code: {}", a.code.map_or_else(|| "None".to_owned(), |a| {format!("{}", a)}));
                    println!("stdout: {}",
                        match convert_to_utf8(&a.stdout) {
                            Ok(s) => s.to_owned(),
                            Err(e) => format!("{:?}", e)
                        }
                    );
                    println!("stderr: {}",
                        match convert_to_utf8(&a.stderr) {
                            Ok(s) => s.to_owned(),
                            Err(e) => format!("{:?}", e)
                        }
                    );
                    println!("}}");
                });
                
        //    println!("{:?}", failed_list);
        }
        _ => {}
    }
}
