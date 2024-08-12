mod args;
mod device;
mod error;
mod screen_reader;
mod volume;

use std::{
	env,
	num::IntErrorKind,
	process::exit,
};

use self::{
	device::{
		Device,
		DeviceState,
	},
	error::Result,
	volume::Volume,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn help_and_exit() {
	println!(
		r#"wol {VERSION}
Show or modify the system volume levels

USAGE: wol [OPTIONS] [ADJUSTMENT...]

OPTIONS:
  -d, --device=<name>: Specify a device name; the string will be matched as a substring case-insensitively
  -i, --id=<id>: Specify a device ID
  -l, --list: Show a list of audio output devices
  -f, --force: If a screen reader is running and the volume goes below 5%, do not refuse to apply the new volume
  -n, --dry-run: Do not actually apply the changes
  -q, --quiet: After modifications, do not print the new volume levels
  -h, --help: Show help
  -V, --version: Show version information

ADJUSTMENT:
  Adjustments have the syntax <channel><operation><value>

    <channel> is optional and can be one of
    - 'L': Left channel
    - 'R': Right channel
    - 'M': Master
    - 'A': All channels
    - <N>: Channel N where N is between 0 and 2^32

  <operation> can be one of '+' (increase volume), '-' (decrease volume) or '=' (set volume)

    <value> Must be one of
    - An integer from 0 to 100
    - 'L': the left channel's current volume
    - 'R': the right channel's current volume
    - 'M': current master volume
    - "c<N>" where <N> is an integer from 0 to 2^32: channel N's current volume

  If the <channel> value is not provided, the operation is done on the master volume level

  As a shorthand to set the master volume, you can omit both <channel> and <operation>
  E.g. "100" (set master volume to max)
    As another shorthand to set the levels for channels 'L', 'R', 'A' or 'M', you can omit the = sign
    E.g. "L40" (Set left channel to 40)"#
	);

	exit(0)
}

fn err_exit<T: std::fmt::Display, O>(msg: T) -> O {
	eprintln!("error: {msg}");
	exit(1);
}

#[derive(Copy, Clone)]
enum Op {
	Set,
	Inc,
	Dec,
}

#[derive(Copy, Clone)]
enum Channel {
	Master,
	All,
	N(u32),
}

#[derive(Copy, Clone)]
enum Value {
	N(u8),
	MasterChannel,
	Channel(u32),
}

#[derive(Copy, Clone)]
struct Adjust {
	op: Op,
	chan: Channel,
	val: Value,
}

impl Value {
	fn parse(s: &str) -> Result<Self, &'static str> {
		let x = match s {
			"m" | "M" => Self::MasterChannel,
			"l" | "L" => Self::Channel(0),
			"r" | "R" => Self::Channel(1),
			_ => {
				if let Some(s) = s.strip_prefix(['c', 'C']) {
					let n = s.parse::<u32>().map_err(|e| match e.kind() {
						IntErrorKind::Empty => "missing a channel number after 'c'",
						IntErrorKind::Zero => unreachable!(),
						_ => "expected an channel number as an integer from 0 to 2^32 after 'c'",
					})?;

					Self::Channel(n)
				} else {
					Self::N(s.parse::<u8>().map_err(|e| match e.kind() {
						IntErrorKind::Empty => "missing a value",
						IntErrorKind::Zero => unreachable!(),
						_ => "the value must be an integer from 0 to 100",
					})?)
				}
			}
		};

		Ok(x)
	}
}

impl Channel {
	fn parse(s: &str) -> Result<Self, &'static str> {
		Ok(match s {
			"l" | "0" | "L" => Self::N(0),
			"r" | "1" | "R" => Self::N(1),
			"a" | "A" => Self::All,
			"" | "m" | "M" => Self::Master,
			_ => return Err("the channel value must be one of 'L', 'R', 'A', 'M' or an integer between 0 and 2^32"),
		})
	}
}

impl Adjust {
	fn parse(s: &str) -> Result<Self, &'static str> {
		let Some(i) = s.find(['+', '-', '=']) else {
			let (chan, s) = s
				.strip_prefix(['L', 'l'])
				.map(|s| (Channel::N(0), s))
				.or_else(|| s.strip_prefix(['R', 'r']).map(|s| (Channel::N(1), s)))
				.or_else(|| s.strip_prefix(['a', 'A']).map(|s| (Channel::All, s)))
				.or_else(|| s.strip_prefix(['m', 'M']).map(|s| (Channel::Master, s)))
				.unwrap_or((Channel::Master, s));

			let val = Value::parse(s)?;
			return Ok(Self {
				op: Op::Set,
				chan,
				val,
			});
		};

		let op = match &s[i..i + 1] {
			"+" => Op::Inc,
			"-" => Op::Dec,
			"=" => Op::Set,
			_ => unreachable!(),
		};

		let chan = Channel::parse(&s[..i])?;
		let val = Value::parse(&s[i + 1..])?;

		Ok(Self { op, chan, val })
	}

	fn apply(self, vol: &mut Volume) {
		let val = match self.val {
			Value::N(n) => n as f32 / 100.0,
			Value::MasterChannel => vol.master(),
			Value::Channel(c) => vol.channel(c),
		};

		let new = move |old| match self.op {
			Op::Set => val,
			Op::Inc => f32::clamp(old + val, 0.0, 1.0),
			Op::Dec => f32::clamp(old - val, 0.0, 1.0),
		};

		match self.chan {
			Channel::Master => {
				let old = vol.master();
				vol.set_master(new(old));
			}
			Channel::N(c) => {
				let old = vol.channel(c);
				vol.set_channel(c, new(old));
			}
			Channel::All => {
				for c in 0..vol.chan_count() {
					let old = vol.channel(c);
					vol.set_channel(c, new(old));
				}
			}
		}
	}
}

struct Args {
	device: Option<String>,
	id: Option<String>,
	force: bool,
	dry: bool,
	quiet: bool,
	adjusts: Vec<Adjust>,
}

fn parse_args() -> Args {
	let argv = env::args()
		.skip(1)
		.filter(|s| !s.is_empty())
		.collect::<Vec<_>>();
	let mut args = args::preprocess(&argv, "di");

	let mut x = Args {
		quiet: false,
		force: false,
		dry: false,
		id: None,
		device: None,
		adjusts: Vec::new(),
	};

	while let Some(s) = args.next() {
		if s == "--" {
			for s in &mut args {
				match Adjust::parse(&s) {
					Ok(a) => x.adjusts.push(a),
					Err(e) => {
						eprintln!("error: failed to parse {s}: {e}");
						exit(1);
					}
				}
			}
			break;
		}

		match &*s {
			"-h" | "--help" => help_and_exit(),
			"-V" | "--version" => {
				println!("wol {VERSION}");
				exit(0);
			}
			"-l" | "--list" => {
				for dev in Device::enumerate(DeviceState::ACTIVE | DeviceState::DISABLED)
					.unwrap_or_else(err_exit)
				{
					let name = dev.name();
					let channels = dev
						.channels()
						.map(|n| format!("; {n} Channels"))
						.unwrap_or_default();

					let id = dev
						.id()
						.ok()
						.filter(|id| !id.is_null())
						.and_then(|id| unsafe { id.to_string().ok() })
						.map_or(String::new(), |id| format!("; ID: {id}"));

					println!("{name}: {state}{channels}{id}", state = dev.state());
				}

				exit(0);
			}
			"-f" | "--force" => x.force = true,
			"-n" | "--dry" => x.dry = true,
			"-q" | "--quiet" => x.quiet = true,
			"-d" | "--device" => {
				x.device = Some(
					args.next()
						.unwrap_or_else(|| err_exit("missing a value for -d --device"))
						.into(),
				);
			}
			"-i" | "--id" => {
				x.id = Some(
					args.next()
						.unwrap_or_else(|| err_exit("missing a value for -i --id"))
						.into(),
				)
			}
			_ => {
				if s.strip_prefix('-')
					.is_some_and(|rest| !rest.starts_with(|c: char| c.is_ascii_digit()))
				{
					eprintln!("error: unknown option {s}");
					exit(1);
				}

				match Adjust::parse(&s) {
					Ok(a) => x.adjusts.push(a),
					Err(e) => {
						eprintln!("error: failed to parse {s}: {e}");
						exit(1);
					}
				}
			}
		}
	}

	x
}

fn run() -> Result<()> {
	let args = parse_args();

	let dev = match (&args.device, &args.id) {
		(None, None) => Device::get_default()?,
		(Some(name), None) => {
			let s = name.to_uppercase();

			let mut devices = Device::enumerate(DeviceState::ACTIVE | DeviceState::DISABLED)?
				.filter(|d| d.name().to_uppercase().contains(&s))
				.collect::<Vec<_>>();

			match &*devices {
				[_] => devices.pop().unwrap(),
				[] => err_exit(format_args!("no such device: {}", name)),
				_ => {
					eprintln!("error: ambiguous device name '{name}'; multiple matches found:");
					for dev in &devices {
						eprintln!("{}", dev.name());
					}
					exit(1);
				}
			}
		}
		(_, Some(id)) => Device::enumerate(DeviceState::ACTIVE | DeviceState::DISABLED)?
			.find(|dev| {
				dev.id()
					.ok()
					.filter(|s| !s.is_null())
					.and_then(|s| unsafe { s.to_string() }.ok())
					.is_some_and(|s| s.eq_ignore_ascii_case(id))
			})
			.ok_or("no active device found with the provided ID")?,
	};

	let mut vol = Volume::new(dev)?;
	let chan_count = vol.chan_count();

	for a in &args.adjusts {
		if let Channel::N(c) = a.chan {
			if c >= chan_count {
				return Err(format!("the device only has {chan_count} channels").into());
			}
		}
	}

	for a in &args.adjusts {
		a.apply(&mut vol);
	}

	if !args.dry && !args.adjusts.is_empty() {
		vol.commit(args.force)?;
	}

	if !args.quiet {
		println!("master: {:.0}", vol.master() * 100.0);

		match chan_count {
			0 | 1 => (),
			2 => {
				println!(
					"balance: {:.0}/{:.0}",
					vol.channel(0) * 100.0,
					vol.channel(1) * 100.0
				);
			}
			_ => {
				for (c, &val) in vol.channels().iter().enumerate() {
					println!("ch{}: {:.0}", c, val * 100.0);
				}
			}
		}
	}

	Ok(())
}

fn main() {
	if let Err(e) = run() {
		eprintln!("error: {e}");
		exit(1);
	}
}
