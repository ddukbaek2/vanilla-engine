//==============================================================================
//! 운영체제 그림 이모티콘 창. (크롬 편집 칸 문맥 메뉴의 "그림 이모티콘" 이 부르는 ui::ShowEmojiPanel 자리)
//! - 맥: [NSApp orderFrontCharacterPalette:nil] (크롬 ui/base/emoji/emoji_panel_helper_mac.mm 과 같다)
//! - 윈도우: SendInput 으로 윈도우 키 + '.' 의 누름과 뗌을 보낸다. 윈도우 10 RS4 부터 이모티콘 창이 뜬다. (emoji_panel_helper_win.cc 와 같다)
//! - 리눅스: 크롬처럼 없다. (emoji_panel_helper.cc 의 IsEmojiPanelSupported 가 false, 메뉴에도 항목이 없다)
//==============================================================================

//==============================================================================
/// 이모티콘 창이 있는지. (크롬 IsEmojiPanelSupported, 윈도우는 판을 보지 않고 참으로 둔다)
//==============================================================================
pub fn is_emoji_panel_supported() -> bool {
	cfg!(any(target_os = "macos", target_os = "windows"))
}

//==============================================================================
/// 이모티콘 창 띄우기. (고른 글자는 IME 처럼 확정 글로 온다)
//==============================================================================
pub fn show_emoji_panel() {
	#[cfg(target_os = "macos")]
	{
		use objc2::msg_send;
		use objc2::runtime::{AnyClass, AnyObject};
		let application_class = match AnyClass::get(c"NSApplication") {
			Some(application_class) => {
				application_class
			}
			None => {
				return;
			}
		};
		unsafe {
			let application: *mut AnyObject = msg_send![application_class, sharedApplication];
			if application.is_null() {
				return;
			}
			let _: () = msg_send![application, orderFrontCharacterPalette: std::ptr::null_mut::<AnyObject>()];
		}
	}
	#[cfg(target_os = "windows")]
	{
		use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LWIN, VK_OEM_PERIOD};
		let create_key_input = |virtual_key: u16, flags: u32| -> INPUT {
			INPUT {
				r#type: INPUT_KEYBOARD,
				Anonymous: INPUT_0 {
					ki: KEYBDINPUT {
						wVk: virtual_key,
						wScan: 0,
						dwFlags: flags,
						time: 0,
						dwExtraInfo: 0,
					},
				},
			}
		};
		let input_list = [create_key_input(VK_LWIN, 0), create_key_input(VK_OEM_PERIOD, 0), create_key_input(VK_LWIN, KEYEVENTF_KEYUP), create_key_input(VK_OEM_PERIOD, KEYEVENTF_KEYUP)];
		unsafe {
			SendInput(input_list.len() as u32, input_list.as_ptr(), std::mem::size_of::<INPUT>() as i32);
		}
	}
}
