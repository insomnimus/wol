mod args;
mod device;
mod error;

use std::{
	env,
	num::IntErrorKind,
	process::exit,
	ptr,
};

use windows::{
	core::Result as WinResult,
	Win32::{
		Media::Audio::{
			Endpoints::IAudioEndpointVolume,
			*,
		},
		System::Com::*,
	},
};

use self::error::Result;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn help_and_exit() {
	println!(
		r#"wol {VERSION}
Show or modify the system volume levels

USAGE: wol [OPTIONS] [ADJUSTMENT...]

OPTIONS:
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

	unsafe fn apply(self, chan_count: u32, dev: &IAudioEndpointVolume) -> WinResult<()> {
		let val = match self.val {
			Value::N(n) => n as f32 / 100.0,
			Value::MasterChannel => dev.GetMasterVolumeLevelScalar()?,
			Value::Channel(c) => dev.GetChannelVolumeLevelScalar(c)?,
		};

		let new = move |old| match self.op {
			Op::Set => val,
			Op::Inc => f32::clamp(old + val, 0.0, 1.0),
			Op::Dec => f32::clamp(old - val, 0.0, 1.0),
		};

		match self.chan {
			Channel::Master => {
				let old = dev.GetMasterVolumeLevelScalar()?;
				dev.SetMasterVolumeLevelScalar(new(old), ptr::null())?;
			}
			Channel::N(c) => {
				let old = dev.GetChannelVolumeLevelScalar(c)?;
				dev.SetChannelVolumeLevelScalar(c, new(old), ptr::null())?;
			}
			Channel::All => {
				for c in 0..chan_count {
					let old = dev.GetChannelVolumeLevelScalar(c)?;
					dev.SetChannelVolumeLevelScalar(c, new(old), ptr::null())?;
				}
			}
		}

		Ok(())
	}
}

unsafe fn default_output() -> WinResult<IAudioEndpointVolume> {
	CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
	let mm_enum: IMMDeviceEnumerator =
		CoCreateInstance(&MMDeviceEnumerator as *const _, None, CLSCTX_ALL)?;

	let device = mm_enum.GetDefaultAudioEndpoint(eRender, eConsole)?;
	device.Activate(CLSCTX_ALL, None)
}

struct Args {
	quiet: bool,
	adjusts: Vec<Adjust>,
}

fn parse_args() -> Args {
	let argv = env::args()
		.skip(1)
		.filter(|s| !s.is_empty())
		.collect::<Vec<_>>();
	let mut args = args::preprocess(&argv, "");

	let mut x = Args {
		quiet: false,
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
			"-q" | "--quiet" => x.quiet = true,
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

unsafe fn run() -> Result<()> {
	let args = parse_args();

	let dev = default_output()?;
	let chan_count = dev.GetChannelCount()?;

	for a in &args.adjusts {
		if let Channel::N(c) = a.chan {
			if c >= chan_count {
				return Err("the device only has {chan_count} channels".into());
			}
		}
	}

	for a in &args.adjusts {
		a.apply(chan_count, &dev)?;
	}

	if !args.quiet || args.adjusts.is_empty() {
		let master = dev.GetMasterVolumeLevelScalar()? * 100.0;
		println!("master: {master:.0}");
		if chan_count == 2 {
			println!(
				"balance: {:.0}/{:.0}",
				dev.GetChannelVolumeLevelScalar(0)? * 100.0,
				dev.GetChannelVolumeLevelScalar(1)? * 100.0,
			);
		} else {
			for channel in 0..chan_count {
				println!(
					"ch{channel}: {level:.0}",
					level = dev.GetChannelVolumeLevelScalar(channel)? * 100.0
				);
			}
		}

		return Ok(());
	}

	Ok(())
}

fn main() {
	unsafe {
		if let Err(e) = run() {
			eprintln!("error: {e}");
			exit(1);
		}
	}
}
