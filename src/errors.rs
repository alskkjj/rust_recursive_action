
use snafu::{Backtrace, Location, prelude::*};

#[derive(Snafu, Debug)]
#[snafu(visibility(pub))]
pub(crate) enum Error {
    Canonilizing {
        source: std::io::Error,
        backtrace: Backtrace,
        #[snafu(implicit)]
        loc: Location,
        dynamic_errmsg: String,
    },
    AtleastOneInStack {
        backtrace: Backtrace,
        #[snafu(implicit)]
        loc: Location,
        dynamic_errmsg: String,
    },
    ReadDir {
        source: std::io::Error,
        backtrace: Backtrace,
        #[snafu(implicit)]
        loc: Location,
        dynamic_errmsg: String,
    },
    DirEntry {
        source: std::io::Error,
        backtrace: Backtrace,
        #[snafu(implicit)]
        loc: Location,
        dynamic_errmsg: String,
    },
    PathFileName {
        backtrace: Backtrace,
        #[snafu(implicit)]
        loc: Location,
        dynamic_errmsg: String,
    },
    CurrentDir {
        source: std::io::Error,
        backtrace: Backtrace,
        #[snafu(implicit)]
        loc: Location,
    },
    ProcessExit {
        code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    LanguageIO {
        source: std::io::Error,
        backtrace: Backtrace,
        #[snafu(implicit)]
        loc: Location,
    },
    NotFoundLanguageFiles {
        backtrace: Backtrace,
        #[snafu(implicit)]
        loc: Location,
        file_location: String
    },
    LanguageNegotiated {
        desired_dirname: String,
        available_langs: Vec<String>
    },
}

pub(crate) type Result<T> = std::result::Result<T, self::Error>;

