
use fluent::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;
use sys_locale;

use std::{io, env};
use std::path::PathBuf;
use std::sync::{Mutex, Arc};
use std::io::Read;
use std::fs;


use snafu::prelude::*;
use crate::errors::*;

pub fn language_matches_score(l1: &LanguageIdentifier, l2: &LanguageIdentifier) -> u8 {
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
    -> Result<Vec<LanguageDeductionHelperS>> {
        if !lang_dir.exists() || !lang_dir.is_dir() {
            return Err(NotFoundLanguageFilesSnafu {
                file_location: lang_dir
                    .to_string_lossy()
                    .into_owned()
            }.build());
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
            Err(
                LanguageNegotiatedSnafu {
                    desired_dirname,
                    available_langs: available_langs
                        .into_iter()
                        .map(|a| { a.lang_name })
                        .collect::<Vec<String>>()
                }.build()
            )
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

static ENV_LANGUAGES_LOCATION: &'static str = "RUST_RECURSIVELY_ACTION_PATH";
#[cfg(target_os = "linux")]
static ENV_APP_INSTALLATION_LOC: &'static str = ".local/share/rust_recursive_action";

fn check_lang_dir(dir_str: &str) -> PathBuf {
    let lang_dir_splitted = dir_str.split(std::path::MAIN_SEPARATOR_STR);

    // If in current dir
    {
        let mut tmp = env::current_dir().expect("Get current dir failed.");
        tmp.extend(lang_dir_splitted.clone());
        if tmp.exists() {
            return tmp;
        }
    }

    // If setted in environment variable
    {
        if let Ok(rust_recursively_action_path) = env::var(ENV_LANGUAGES_LOCATION) {
            use std::str::FromStr;
            #[allow(irrefutable_let_patterns)]
            if let Ok(mut lang_dir) = PathBuf::from_str(&rust_recursively_action_path) {
                lang_dir.extend(lang_dir_splitted.clone());
                if lang_dir.exists() {
                    return lang_dir;
                }
            }
        }
    }

    // If `i18n/fluent` is at home directoru
    {
        if let Some(mut hd) = env::home_dir() {
            hd.extend(ENV_APP_INSTALLATION_LOC.split(std::path::MAIN_SEPARATOR));
            hd.extend(lang_dir_splitted.clone());
            if hd.exists() {
                return hd;
            }
        }
    }

    unimplemented!("Can't find the language installation directory. Since there's no a installation process\nPut the `i18n` directory into `.local/share/rust_recursive_action`\nor set the env variable `RUST_RECURSIVELY_ACTION_PATH` to where i18n located.");
}

impl LanguageSystem {
    pub fn new(desired_lang: Option<String>, lang_dir: Option<String>) -> Self {
        let lang_dir = lang_dir.unwrap_or("i18n/fluent".to_string());
        let lang_dir = check_lang_dir(&lang_dir);

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
                    let path_extension = path.extension()
                        .expect("File extension is not correct")
                        .to_string_lossy();
                    if path.is_file() && path.extension().is_some()
                        && path_extension == "ftl" {
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

pub fn build_language_0<'a>(msg_key: &str) -> String {
    match LANG.get()
        .expect("Uninitialized language bundle.").lock() {
        Ok(bs) => {

            let expect_errmsg = format!("failed to find message {}", msg_key);
            let msg = bs.bundle
                .get_message(msg_key)
                .expect(&expect_errmsg);
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

pub fn build_language_1<'a, T>(msg_key: &str, arg_name: &str, v: T) -> String
    where T: Into<FluentValue<'a>> {
    build_language(msg_key,
        vec![(arg_name, v.into())])
}

pub fn build_language_fns<'a, F>(msg_key: &str, args_pairs_builders: Vec<(&str, F)>) -> String
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


pub fn build_language<'a>(msg_key: &str, args_pairs: Vec<(&str, FluentValue)>) -> String {
    match LANG.get() {
        Some(lang) => {
            match lang.lock() {
                Ok(bs) => {
                    let expect_errmsg = format!("failed to find message {}", msg_key);
                    let msg = bs
                        .bundle
                        .get_message(msg_key)
                        .expect(&expect_errmsg);

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
                },
                Err(e) => {
                    panic!("Language bundle mutex poisoned {e:?}")
                }
            }
        },
        None => {
            panic!("Uninitialized lang bundle")
        }
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



