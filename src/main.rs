mod errors;
mod language_helpers;

use std::fs;
use std::path::PathBuf;
use std::{env, };

use std::process::Command;
use clap::{Parser, ValueEnum, builder::PossibleValue};
use fluent::FluentValue;
use snafu::prelude::*;

use strum::IntoStaticStr;

use language_helpers as lh;
use errors::*;

fn get_cargo_directories(path_str: &str) -> Result<Vec<PathBuf>> {
    let mut dir_pathes = Vec::<PathBuf>::new();
    // save the last index of every directory's subitems in dir_pathes
    let mut dir_sizes = Vec::<usize>::new();

    let mut marked_pathes = Vec::<PathBuf>::new();

    let path = fs::canonicalize(&path_str)
        .context(CanonilizingSnafu {
            dynamic_errmsg:
                lh::build_language_fns(
                    "file-path-canonicalized-failed",
                    vec![(
                        "path_dir", || {
                            fluent::FluentValue::from(path_str)
                        }
                    )])
        })?;

    dir_pathes.push(path);
    dir_sizes.push(1);

    loop {
        let mut marked_cargo_dir = false;
        if dir_pathes.is_empty() { break; }

        let dir_path = dir_pathes
            .last()
            .context(AtleastOneInStackSnafu {
                dynamic_errmsg:
                    "The algorithm has logical bugs if unwrap fails."
                    .to_string()
            })?;

        let sub_items = {
            let dir_iter = fs::read_dir(dir_path)
            .context(ReadDirSnafu {
                dynamic_errmsg: lh::build_language_1(
                                    "read-directory-failed",
                                    "dir_path",
                                    dir_path.to_str()
                                )})?;
            let mut ps = vec![];
            for dirent in dir_iter {
                let p = dirent.context(
                    DirEntrySnafu {
                        dynamic_errmsg: lh::build_language_0("read-directory-entry-failed")
                    })?
                    .path();
                ps.push(p);
            }
            ps
        };

        if sub_items.iter().any(|a| {
            "Cargo.toml" == a.as_path().file_name()
                .expect(&lh::build_language_0("get-file-name-failed"))
                .to_str()
                .expect(
                    &lh::build_language_0("osstring-to-string-failed"))
        }) {
           marked_pathes.push(dir_pathes.last().unwrap().clone());
           marked_cargo_dir = true;
        }

        let sub_items = sub_items.iter()
            .filter(|a| {
                // filter the directories that name start with . out
                // meanwhile filter the no-directories out.
                !a.as_path()
                    .file_name()
                    .expect(&lh::build_language_0("get-file-name-failed"))
                    .to_str()
                    .expect(
                        &lh::build_language_0("osstring-to-string-failed"))
                    .starts_with(".")
                    && a.metadata()
                    .expect(&lh::build_language_0("get-metadata-error"))
                    .is_dir()
            })
        .filter(|a| {
            // excluding the `target` and `src` directories
            let file_name = a.as_path().file_name()
                .expect(&lh::build_language_0("get-file-name-failed"))
                .to_str()
                .expect(&lh::build_language_0("osstring-to-string-failed"));
            if marked_cargo_dir {
                file_name != "target" && file_name != "src"
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

    return Ok(marked_pathes);
}

#[derive(PartialEq, Debug, Default, Clone, Copy, Eq, PartialOrd, Ord, )]
enum GeneratingType {
    #[default]
    BashCommands,
    RunAsSubprocess,
    DryRunDebug,
}

impl ValueEnum for GeneratingType {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::BashCommands, Self::RunAsSubprocess, Self::DryRunDebug]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Self::BashCommands => {
                PossibleValue::new("bash-commands")
                    .help(
                        &lh::build_language_0("generate-bash-like-cmds-helper"))
                    .aliases(["cmd", "cmds", "bash_cmds", "bash_commands"])
            }
            Self::RunAsSubprocess => {
                PossibleValue::new("run-as-subprocess")
                    .help(
                        &lh::build_language_0("directly-run-helper"))
                    .aliases(["direct", "subprocess", "directly"])
            }
            Self::DryRunDebug => {
                PossibleValue::new("dry-run-debug")
                    .help(&lh::build_language_0("dry-run-helper"))
                    .aliases(["dry_run", "dry-run", "dr"])
            }
        })
    }
}


fn process_dir(cargo_dir: &PathBuf, ge_ty: GeneratingType, subcmd: GeneratingSubcommand, ) -> Result<()> {
    let old_dir = std::env::current_dir()
        .context(CurrentDirSnafu)?;

    let old_dir_str = old_dir.to_str()
        .expect(&lh::build_language_0("pathbuf-to-str-failed"));
    let dest_dir_str = cargo_dir.to_str()
        .expect(&lh::build_language_0("pathbuf-to-str-failed"));

    let subcmd = {
        let tmp: &'static str = subcmd.into();
        let s = String::from(tmp);
        s.to_lowercase()
    };

    match ge_ty {
        GeneratingType::BashCommands => {
            println!("cd {}", dest_dir_str);
            println!("cargo {subcmd}");
            println!("cd {}", old_dir_str);
            Ok(())
        },
        GeneratingType::RunAsSubprocess => {
            env::set_current_dir(cargo_dir)
                    .context(CurrentDirSnafu)?;

            let output = Command::new("cargo").arg(&subcmd)
                .output()
                .expect(&lh::build_language_1("start-cargo-subcommand-failed", "subcommand", subcmd));
            if !output.status.success() {
                env::set_current_dir(old_dir)
                    .context(CurrentDirSnafu)?;

                return Err(Error::ProcessExit {
                    code: output.status.code(),
                    stdout: output.stdout,
                    stderr: output.stderr,
                });
            }
            env::set_current_dir(old_dir)
                .context(CurrentDirSnafu)?;
            Ok(())
        },
        GeneratingType::DryRunDebug => {
            eprintln!("RUN: cargo {subcmd} at {}", dest_dir_str);
            Ok(())
        }
    }
}

#[derive(PartialOrd, PartialEq, Eq, Ord, Debug, ValueEnum, Default, Clone, Copy, IntoStaticStr)]
enum GeneratingSubcommand {
    #[default]
    Clean,
    Update,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    // starting point of directories, would be "./" if it isn't supplied.
    target_dir: Option<String>,
    // generaty types: bash commands(default), output debug(dry run), direct run as subprocess.
    #[arg(long = "gt", value_enum, default_value_t)]
    generating_type: GeneratingType,

    #[arg(long = "gs", value_enum, default_value_t)]
    generating_subcommand: GeneratingSubcommand,

}

fn main() {
    lh::init_lang(None, None);

    let cli = Cli::parse();
    let path_str = cli.target_dir.unwrap_or("./".to_string());
    let ge_ty = cli.generating_type;

    let mut failed_list = Vec::new();

    let marked_pathes =
        match get_cargo_directories(&path_str) {
            Ok(o) => {
                o
            },
            Err(e) => {
                panic!("{:?}", e)
            }
        };

    if ge_ty == GeneratingType::BashCommands
        || ge_ty == GeneratingType::DryRunDebug {
            let msg_key = "root-path";
            let mut args_pairs = vec![];

            let args_pair = (
                    "root_path",
                    FluentValue::from({
                            let t = fs::canonicalize(&path_str)
                                .unwrap();
                            let t = t.as_path()
                                .to_str()
                                .unwrap();
                            t.to_owned()
                        })
                );
            args_pairs.push(args_pair);
            println!("{}", lh::build_language(msg_key, args_pairs));
    }

    marked_pathes
        .iter()
        .for_each(|a| {
            match process_dir(a, ge_ty.clone(), cli.generating_subcommand) {
                Ok(_) => {
                    // printed/start processes in function `process_dir`
                },
                Err(pde) => {
                    match pde {
                        Error::CurrentDir {
                            ..
                        } => {
                            panic!("read/set current dir failed: {:?}", pde);
                        },
                        Error::ProcessExit {
                            ..
                        } => {
                            // record the failed processes
                            failed_list.push(pde);
                        }
                        _ => {
                            panic!("{:?}", pde)
                        }
                    }
                }
            }
    });

    match ge_ty {
        GeneratingType::RunAsSubprocess => {
            failed_list.iter().for_each(|a| {
                match a {
                    Error::ProcessExit {
                        code,
                        stdout,
                        stderr,
                    } => {
                        println!("{{");
                        println!("  code: {}", code.map_or_else(|| "None".to_owned(), |a| {format!("{}", a)}));
                        println!("  stdout: {:?}", String::from_utf8_lossy(stdout));
                        println!("  stderr: {:?}", String::from_utf8_lossy(stderr));
                        println!("}}");
                    }
                    _ => {
                        // do nothing
                    }
                }
            });
        }
        _ => {}
    }
}
