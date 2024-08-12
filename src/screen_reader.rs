use windows::Win32::{
	Foundation::BOOL,
	UI::WindowsAndMessaging::{
		SystemParametersInfoA,
		SPI_GETSCREENREADER,
		SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
	},
};

pub fn is_running() -> bool {
	unsafe {
		let mut yes = BOOL(0);
		let ok = SystemParametersInfoA(
			SPI_GETSCREENREADER,
			0,
			Some(&mut yes as *mut _ as *mut _),
			SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
		)
		.is_ok();

		ok && yes.as_bool()
	}
}
