use windows::core::Result as WinResult;

use crate::{
	device::Device,
	error::Result,
	screen_reader,
};

pub struct Volume {
	dev: Device,
	master: f32,
	channels: Vec<f32>,
	init_master: f32,
	init_channels: Vec<f32>,
}

impl Volume {
	pub fn new(dev: Device) -> WinResult<Self> {
		let master = dev.master_volume()?;
		let n_chan = dev.channels()?;
		let mut channels = Vec::with_capacity(n_chan as usize);
		for i in 0..n_chan {
			channels.push(dev.channel_volume(i)?);
		}

		Ok(Self {
			dev,
			init_master: master,
			init_channels: channels.clone(),
			master,
			channels,
		})
	}

	pub fn set_channel(&mut self, c: u32, val: f32) {
		let val = val.clamp(0.0, 1.0);
		self.channels[c as usize] = val;
		self.master = self
			.channels
			.iter()
			.copied()
			.max_by(f32::total_cmp)
			.unwrap_or(self.master);
	}

	pub fn set_master(&mut self, val: f32) {
		let val = val.clamp(0.0, 1.0);
		if val == 0.0 {
			self.master = 0.0;
			self.channels.iter_mut().for_each(|n| *n = 0.0);
			return;
		} else if self.master == 0.0 {
			self.master = val;
			self.channels.iter_mut().for_each(|n| *n = val);
			return;
		}

		for n in &mut self.channels {
			let ratio = *n / self.master;
			*n = val * ratio;
		}

		self.master = val;
	}

	pub fn chan_count(&self) -> u32 {
		self.channels.len() as u32
	}

	pub fn master(&self) -> f32 {
		self.master
	}

	pub fn channel(&self, c: u32) -> f32 {
		self.channels[c as usize]
	}

	pub fn channels(&self) -> &[f32] {
		&self.channels
	}

	pub fn commit(&self, force: bool) -> Result<()> {
		// Try not to set the volume below 5% for people that use a screen reader.
		if !force && self.master < self.init_master && self.master < 0.05 {
			let old_max = self
				.init_channels
				.iter()
				.copied()
				.max_by(f32::total_cmp)
				.unwrap_or(1.0);
			let new_max = self
				.channels
				.iter()
				.copied()
				.max_by(f32::total_cmp)
				.unwrap_or(1.0);
			if new_max < old_max && new_max < 0.05 && screen_reader::is_running() {
				return Err("a screen reader is detected; refusing to set the volume below 5%\nhint: use --force to override this behaviour".into());
			}
		}

		let master_changed = self.master != self.init_master;
		if master_changed {
			self.dev.set_master_volume(self.master)?;
		}

		for (i, (&old, &new)) in self
			.init_channels
			.iter()
			.zip(self.channels.iter())
			.enumerate()
		{
			if master_changed || old != new {
				self.dev.set_channel_volume(i as u32, new)?;
			}
		}

		Ok(())
	}
}
