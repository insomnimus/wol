use std::{
	cell::OnceCell,
	ptr,
};

use derive_more::derive::{
	BitAnd,
	BitAndAssign,
	BitOr,
	BitOrAssign,
	From,
	Into,
};
use windows::{
	core::{
		Result,
		PWSTR,
	},
	Win32::{
		Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
		Media::Audio::{
			Endpoints::IAudioEndpointVolume,
			*,
		},
		System::{
			Com::*,
			Variant::*,
		},
	},
};

#[derive(Debug)]
pub struct Devices {
	len: u32,
	cur: u32,
	collection: IMMDeviceCollection,
}

#[derive(Debug, Clone)]
pub struct Device {
	name: String,
	dev: IMMDevice,
	vol: OnceCell<IAudioEndpointVolume>,
}

#[derive(
	Copy,
	Clone,
	Debug,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Hash,
	From,
	Into,
	BitAnd,
	BitAndAssign,
	BitOr,
	BitOrAssign,
)]
pub struct DeviceState(pub u32);

impl DeviceState {
	pub const ACTIVE: Self = Self(DEVICE_STATE_ACTIVE.0);
	pub const ANY: Self =
		Self(Self::ACTIVE.0 | Self::DISABLED.0 | Self::NOT_PRESENT.0 | Self::UNPLUGGED.0);
	pub const DISABLED: Self = Self(DEVICE_STATE_DISABLED.0);
	pub const NOT_PRESENT: Self = Self(DEVICE_STATE_NOTPRESENT.0);
	pub const UNPLUGGED: Self = Self(DEVICE_STATE_UNPLUGGED.0);

	pub const fn has(self, flag: Self) -> bool {
		self.0 | flag.0 == self.0
	}
}

impl Iterator for Devices {
	type Item = Device;

	fn size_hint(&self) -> (usize, Option<usize>) {
		let n = u32::saturating_sub(self.len, self.cur) as usize;
		(n, Some(n))
	}

	fn count(self) -> usize {
		u32::saturating_sub(self.len, self.cur) as usize
	}

	fn nth(&mut self, n: usize) -> Option<Self::Item> {
		if n >= self.len as usize {
			return None;
		}

		self.cur = n as u32;
		self.next()
	}

	fn next(&mut self) -> Option<Self::Item> {
		if self.cur >= self.len {
			None
		} else {
			let x = unsafe {
				self.collection
					.Item(self.cur)
					.and_then(|dev| Device::new(dev))
					.ok()
			};
			self.cur += 1;
			x
		}
	}
}

impl Device {
	unsafe fn new(dev: IMMDevice) -> Result<Self> {
		let vol = OnceCell::new();
		let props = dev.OpenPropertyStore(STGM_READ)?;
		let varname = props
			.GetValue(&PKEY_Device_FriendlyName)?
			.as_raw()
			.Anonymous;
		if varname.Anonymous.vt == VT_EMPTY.0 {
			return Ok(Self {
				dev,
				vol,
				name: String::new(),
			});
		}

		let name = PWSTR(varname.Anonymous.Anonymous.pwszVal);
		if name.is_null() {
			Ok(Self {
				dev,
				vol,
				name: String::new(),
			})
		} else {
			Ok(Self {
				dev,
				vol,
				name: String::from_utf16_lossy(name.as_wide()),
			})
		}
	}

	unsafe fn enumerator() -> Result<IMMDeviceEnumerator> {
		CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
		let mm_enum: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

		Ok(mm_enum)
	}

	fn vol(&self) -> Result<&IAudioEndpointVolume> {
		if let Some(x) = self.vol.get() {
			return Ok(x);
		}
		let vol = unsafe { self.dev.Activate(CLSCTX_ALL, None)? };
		Ok(self.vol.get_or_init(move || vol))
	}
}

impl Device {
	pub fn get_default() -> Result<Self> {
		unsafe {
			let mm_enum = Self::enumerator()?;
			let dev = mm_enum.GetDefaultAudioEndpoint(eRender, eConsole)?;
			Self::new(dev)
		}
	}

	pub fn enumerate(state: Option<DeviceState>) -> Result<Devices> {
		let state = state.unwrap_or(DeviceState::ANY).0;

		unsafe {
			let enumerator = Self::enumerator()?;
			let enumerator = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE(state))?;

			Ok(Devices {
				cur: 0,
				len: enumerator.GetCount()?,
				collection: enumerator,
			})
		}
	}

	pub fn name(&self) -> &str {
		&self.name
	}

	pub fn channels(&self) -> Result<u32> {
		unsafe { self.vol()?.GetChannelCount() }
	}

	pub fn master_volume(&self) -> Result<f32> {
		unsafe { self.vol()?.GetMasterVolumeLevelScalar() }
	}

	pub fn set_master_volume(&self, volume: f32) -> Result<()> {
		unsafe { self.vol()?.SetMasterVolumeLevelScalar(volume, ptr::null()) }
	}

	pub fn channel_volume(&self, channel: u32) -> Result<f32> {
		unsafe { self.vol()?.GetChannelVolumeLevelScalar(channel) }
	}

	pub fn set_channel_volume(&self, channel: u32, volume: f32) -> Result<()> {
		unsafe {
			self.vol()?
				.SetChannelVolumeLevelScalar(channel, volume, ptr::null())
		}
	}

	pub fn state(&self) -> Result<DeviceState> {
		Ok(DeviceState(unsafe { self.dev.GetState()?.0 }))
	}
}
