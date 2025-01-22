use std::fs;
use std::path::PathBuf;
use std::{env, io};

use std::process::Command;
use std::string::FromUtf8Error;
use clap::{Parser, ValueEnum, builder::PossibleValue};

use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;
use sys_locale;
use strum::IntoStaticStr;

use std::sync::{Mutex, Arc};
use std::io::Read;


/// fluent functions
#[derive(Debug, )]
enum LanguageChoiceError {
    IoError(io::Error),
    NoLanguageFilesAt(String),
    LanguageNegotiatedFailed(String, Vec<String>, ),
}

impl From<io::Error> for LanguageChoiceError {
    fn from(e: io::Error) -> Self {
        Self::IoError(e)
    }
}

fn language_matches_score(l1: &LanguageIdentifier, l2: &LanguageIdentifier) -> u8 {
    let mut base = 0u8;
    base |= if l1.matches(l2, false, false) { 0b1000 } else { 0 };
    base |= if l1.matches(l2, false, true) { 0b0100 } else { 0 };
    base |= if l1.matches(l2, true, false) { 0b0010 } else { 0 };
    base |= if l1.matches(l2, true, true) { 0b0001 } else { 0 };
    base
}

#[derive(Debug)]
struct LanguageDeductionHelperS {
    pub lid: LanguageIdentifier,
    pub lang_name: String,
    pub dir_path: PathBuf,
    pub score: u8,
}

fn resolve_desired_lang(lang_name: Option<String>, lang_dir: &PathBuf) 
    -> Result<Vec<LanguageDeductionHelperS>, LanguageChoiceError> {
        if !lang_dir.exists() || !lang_dir.is_dir() {
            return Err(LanguageChoiceError::NoLanguageFilesAt(
                    lang_dir.canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
                    ));
        }

        let (desired_lang_identifier, desired_dirname) = match &lang_name {
            Some(lang) => {
                (lang.parse::<LanguageIdentifier>()
                    .expect(&format!("Parse {lang} as language identifier failed.")),
                    lang.clone())
            },
            None => {
                let n = sys_locale::get_locale()
                    .expect(&format!("Get system locale failed."));
                let li = n.clone()
                    .parse::<LanguageIdentifier>()
                    .expect("System's default locale parses failed.");
                (li, n)
            }
        };

        let available_langs = {
            let mut available_langs = Vec::new();
            let read_dir = fs::read_dir(lang_dir)
                .expect(&format!("Read dir {:?} failed.", lang_dir));
            for dir in read_dir {
                let dir_ent = dir.expect(&format!("Read a dir entry in {:?} failed.", lang_dir));
                let dir_path = dir_ent.path();

                let dirname = {
                    let os_name = dir_ent.file_name();
                    os_name.to_str()
                        .expect(&format!("OsString {:?} converts to String failed.", &os_name)).to_owned()
                };
                match &dirname.parse::<LanguageIdentifier>() {
                    Ok(id) => {
                        let tmp = LanguageDeductionHelperS {
                            lid: id.clone(),
                            lang_name: dirname,
                            dir_path,
                            score: language_matches_score(&id, &desired_lang_identifier)
                        };
                        available_langs.push(tmp);
                    },
                    Err(_e) => {
                    }
                }
            }
            available_langs.sort_by_cached_key(|a| { a.lang_name.clone() });
            available_langs.sort_by(|a, b| { b.score.cmp(&a.score) });
            available_langs
        };
        if !available_langs.is_empty() {
            Ok(available_langs)
        } else {
            Err(LanguageChoiceError::LanguageNegotiatedFailed(desired_dirname, available_langs.into_iter()
                    .map(|a| {
                        a.lang_name
                    })
                    .collect()
                    ))
        }
   }

struct LanguageSystem {
    pub bundle: fluent::FluentBundle<FluentResource>,
    pub current_lang: LanguageIdentifier,
    pub current_lang_dir_path: PathBuf,
}

unsafe impl Sync for LanguageSystem {}
unsafe impl Send for LanguageSystem {}

use std::sync::OnceLock;

static LANG: OnceLock<Mutex<Arc<LanguageSystem>>> = OnceLock::new();

impl LanguageSystem {
    pub fn new(desired_lang: Option<String>, lang_dir: Option<String>) -> Self {

        let default_lang_dir_str = "i18n/fluent".to_owned();
        let lang_dir = lang_dir.or(Some(default_lang_dir_str)).unwrap();
        let lang_dir = {
            let mut tmp = env::current_dir()
                .expect("Get current dir failed.");
            tmp.extend(lang_dir.split("/"));
            tmp
        };


        let ordered_langs = resolve_desired_lang(desired_lang.clone(), &lang_dir)
            .expect(&format!("fetch languages {:?} failed.", desired_lang));
        let v = ordered_langs
            .iter()
            .map(|a| { a.lid.clone() })
            .collect();
        let mut bundle = FluentBundle::new(v);
            
        let desired_lang_helper_s = &ordered_langs.first().unwrap();

        { // add ftl files under desired directory to bundle.
            let read_dir = fs::read_dir(&desired_lang_helper_s.dir_path)
                .expect(&format!("read language dir {:?} failed", &desired_lang_helper_s.dir_path));

            for entry in read_dir {
                if let Ok(dir_entry) = entry {
                    let path = dir_entry.path();
                    if path.is_file() && path.extension().is_some()
                        && path.extension().unwrap() == "ftl" {
                            {
                                let mut f = fs::File::open(path)
                                    .expect("failed to open one of ftl files.");
                                let mut s = String::new();
                                f.read_to_string(&mut s).expect("read ftl file to string failed.");
                                let r = FluentResource::try_new(s)
                                    .expect("Could not parse an FTL string.");
                                bundle.add_resource(r)
                                    .expect("Failed to add FTL resources to the bundle.");
                                }
                    }
                }
            }
        }

        Self {
            bundle,
            current_lang: desired_lang_helper_s.lid.clone(),
            current_lang_dir_path: desired_lang_helper_s.dir_path.clone(),
        }
    }
}

fn build_language_0<'a>(msg_key: &str) -> String {
    match LANG.get()
        .expect("Uninitialized language bundle.").lock() {
        Ok(bs) => {
            
            let msg = bs.bundle
                .get_message(msg_key)
                .expect(&format!("failed to find message {msg_key}"));
            let mut errors = vec![];
            let pattern = msg.value()
                .expect("Message has no value.");
            let v = bs.bundle.format_pattern(pattern, None, &mut errors);
            v.to_string()
        },
        Err(e) => {
            panic!("Language bundle mutext poisoned. {e}");
        }
    }
}

fn build_language_1<'a, T>(msg_key: &str, arg_name: &str, v: T) -> String
    where T: Into<FluentValue<'a>> {
    build_language(msg_key, 
        vec![(arg_name, v.into())])
}

fn build_language_fns<'a, F>(msg_key: &str, args_pairs_builders: Vec<(&str, F)>) -> String 
where F: FnOnce() -> FluentValue<'a>{
    let args_pairs: Vec<_> = args_pairs_builders.into_iter()
        .map(
            |a| {
            (a.0,
             a.1())
            }
            )
        .collect();
    build_language(msg_key, args_pairs)
}


fn build_language<'a>(msg_key: &str, args_pairs: Vec<(&str, FluentValue)>) -> String {
    if let Ok(bs) = LANG.get().expect("Uninitialized language bundle.").lock() {
        let msg = bs
            .bundle
            .get_message(msg_key)
            .expect("failed to find message {msg_key}");

        let pattern = msg.value()
            .expect("Message has no value");

        let mut args  = FluentArgs::new();
        for kv in args_pairs {
            args.set(kv.0, 
                kv.1);
        }

        let mut errors = vec![];
        let value = bs.bundle.format_pattern(pattern, Some(&args), &mut errors);
        value.to_string()
    } else {
        panic!("Language bundle mutex poisoned")
    }
}

pub fn init_lang(desired_lang: Option<String>, lang_dir: Option<String>) {
    if LANG
        .set(Mutex::new(
                Arc::new(LanguageSystem::new(desired_lang, lang_dir))))
            .is_err()  {
                panic!("set initialized Language system failed.");
    }
}



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

    let path = std::fs::canonicalize(&path_str);
    if path.is_err() {
        panic!("{}", &build_language_fns("file-path-canonized-failed", vec![(
                "path_dir", || {
                    FluentValue::from(path_str)
                }
        )]));
    }

    dir_pathes.push(path.unwrap());
    dir_sizes.push(1);
 
    loop {
        let mut marked_cargo_dir = false;
        if dir_pathes.is_empty() { break; }

        // The algorithm has bugs if unwrap fails.
        let dir_path = dir_pathes
            .last()
            .unwrap();

        let sub_items = fs::read_dir(dir_path)
            .expect(
                &build_language_1("read-directory-failed", "dir_path", 
                    dir_path.to_str()
                    ))
            .map(|a| {
                a.expect(
                    &build_language_0("read-directory-entry-failed")
                    )
                    .path()
            })
        .collect::<Vec<PathBuf>>();


        if sub_items.iter().any(|a| {
            a.as_path().file_name()
                .expect(&build_language_0("get-file-name-failed"))
                .to_str()
                .expect(
                    &build_language_0("osstring-to-string-failed")) == "Cargo.toml"
        }) {
           marked_pathes.push(dir_pathes.last().unwrap().clone());
           marked_cargo_dir = true;
        }

        let sub_items = sub_items.iter()
            .filter(|a| {
                !a.as_path().file_name()
                    .expect(&build_language_0("get-file-name-failed"))
                    .to_str()
                    .expect(
                        &build_language_0("osstring-to-string-failed"))
                    .starts_with(".") 
                    && a.metadata()
                    .expect(&build_language_0("get-metadata-error"))
                    .is_dir()
            })
        .filter(|a| {
            let file_name = a.as_path().file_name()
                .expect(&build_language_0("get-file-name-failed"))
                .to_str()
                .expect(&build_language_0("osstring-to-string-failed"));
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

impl ValueEnum for GeneratingType {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::BashCommands, Self::RunAsSubprocess, Self::DryRunDebug]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Self::BashCommands => {
                PossibleValue::new("bash-commands")
                    .help(
                        &build_language_0("generate-bash-like-cmds-helper"))
                    .aliases(["cmd", "cmds", "bash_cmds", "bash_commands"])
            }
            Self::RunAsSubprocess => {
                PossibleValue::new("run-as-subprocess")
                    .help(
                        &build_language_0("directly-run-helper"))
                    .aliases(["direct", "subprocess", "directly"])
            }
            Self::DryRunDebug => {
                PossibleValue::new("dry-run-debug")
                    .help(&build_language_0("dry-run-helper"))
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

fn process_dir(cargo_dir: &PathBuf, ge_ty: GeneratingType, subcmd: GeneratingSubcommand, ) -> Result<(), ProcessDirError> {
    let old_dir = env::current_dir()?;

    let old_dir_str = old_dir.to_str()
        .expect(&build_language_0("pathbuf-to-str-failed"));
    let dest_dir_str = cargo_dir.to_str()
        .expect(&build_language_0("pathbuf-to-str-failed"));

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
            env::set_current_dir(cargo_dir)?;

            let output = Command::new("cargo").arg(&subcmd)
                .output()
                .expect(&build_language_1("start-cargo-subcommand-failed", "subcommand", subcmd));
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
    init_lang(None, None);

    let cli = Cli::parse();
    let path_str = cli.target_dir.or(Some("./".to_owned())).unwrap();
    let ge_ty = cli.generating_type;

    let mut failed_list = Vec::new();

    let marked_pathes = get_cargo_directories(&path_str);

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
            println!("{}", build_language(msg_key, args_pairs));
    }

    marked_pathes
        .iter()
        .for_each(|a| {
            match process_dir(a, ge_ty.clone(), cli.generating_subcommand) {
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
