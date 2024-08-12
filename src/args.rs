use std::{
	borrow::{
		Cow,
		Cow::{
			Borrowed,
			Owned,
		},
	},
	collections::VecDeque,
};

pub struct Preprocessor<'a> {
	buf: VecDeque<Cow<'a, str>>,
	args: std::slice::Iter<'a, String>,
	double_dash: bool,
	shorts_with_val: &'static str,
}

impl<'a> Iterator for Preprocessor<'a> {
	type Item = Cow<'a, str>;

	fn next(&mut self) -> Option<Self::Item> {
		if let Some(a) = self.buf.pop_front() {
			return Some(a);
		}

		if self.double_dash {
			return self.args.next().map(|s| Borrowed(s.as_str()));
		}

		let s = self.args.next()?;
		if s == "--" {
			self.double_dash = true;
			return self.next();
		}

		if s.starts_with("--") {
			return match s.split_once('=') {
				Some((flag, val)) if flag.len() > 2 => {
					self.buf.push_back(Borrowed(val));
					Some(Borrowed(flag))
				}
				_ => Some(Borrowed(s)),
			};
		}

		if let Some(flags) = s.strip_prefix('-') {
			if flags.is_empty() {
				return Some(Borrowed("-"));
			}

			// Minor optimization to avoid an allocation
			if flags.chars().count() == 1 {
				return Some(Borrowed(s));
			}

			// Ensure that args like `-42` pass through
			if flags.starts_with(|c: char| c.is_ascii_digit()) {
				return Some(Borrowed(s));
			}

			for (i, c) in flags.char_indices() {
				self.buf.push_back(Owned(format!("-{c}")));

				if self.shorts_with_val.contains(c) {
					let val = &flags[i + c.len_utf8()..];
					if !val.is_empty() {
						self.buf.push_back(Borrowed(val));
					}
					break;
				}
			}

			return self.buf.pop_front();
		}

		Some(Borrowed(s))
	}
}

pub fn preprocess<'a>(args: &'a [String], shorts_with_val: &'static str) -> Preprocessor<'a> {
	Preprocessor {
		buf: VecDeque::with_capacity(8),
		args: args.iter(),
		shorts_with_val,
		double_dash: false,
	}
}
