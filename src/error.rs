use std::borrow::Cow;

use derive_more::{
	Display,
	From,
};
use windows::core::Error as WinError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Display, From)]
pub enum Error {
	#[from(Cow<'static, str>, String, &'static str)]
	Str(Cow<'static, str>),
	#[from]
	Win(WinError),
}

impl std::error::Error for Error {}
