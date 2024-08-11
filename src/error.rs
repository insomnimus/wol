use derive_more::{
	Display,
	From,
};
// use std::fmt;
use windows::core::Error as WinError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Display, From)]
pub enum Error {
	Static(&'static str),
	Win(WinError),
}

impl std::error::Error for Error {}

// impl From<WinError> for Error {
// fn from(e: WinError) -> Self {
// Self::Win(e)
// }
// }

// impl From<&'static str> for Error {
// fn from(s: &'static str) -> Self {
// Self::Static(s)
// }
// }

// impl fmt::Display for Error {
// fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
// match self {
// Self::Static(s) => f.write_str(s),
// Self::Win(e) => write!(f, "{e}"),
// }
// }
// }
