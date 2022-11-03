use std::collections::HashMap;
use std::io::{ self, Read as _, Stdin, Stdout, Write };
use std::os::unix::io::AsRawFd as _;
use std::rc::Rc;
use termios::Termios;

use crate::config::*;
use crate::misc::*;

pub struct Terminal {
	stdin: Stdin,
	stdout: Stdout,
	termios: Option <Termios>,
	buf_in: Vec <u8>,
	buf_in_start: usize,
	buf_in_end: usize,
	buf_out: Vec <u8>,
}

impl Terminal {
	pub fn new () -> GenResult <Terminal> {
		Ok (Self {
			stdin: io::stdin (),
			stdout: io::stdout (),
			termios: None,
			buf_in: vec! [0_u8; 1024],
			buf_in_start: 0,
			buf_in_end: 0,
			buf_out: vec! [],
		})
	}
	pub fn start (& mut self) -> GenResult <()> {
		assert! (self.termios.is_none ());
		let mut termios = Termios::from_fd (self.stdin.as_raw_fd ()) ?;
		self.termios = Some (termios);
		termios::cfmakeraw (& mut termios);
		termios::tcsetattr (self.stdin.as_raw_fd (), termios::TCSANOW, & termios) ?;
		write! (self.stdout, "\x1b[?1049h") ?;
		self.stdout.flush () ?;
		Ok (())
	}
	pub fn stop (& mut self) -> GenResult <()> {
		assert! (self.termios.is_some ());
		write! (self, "\x1b[?1049l").unwrap ();
		self.flush ().unwrap ();
		let termios = self.termios.take ().unwrap ();
		termios::tcsetattr (self.stdin.as_raw_fd (), termios::TCSANOW, & termios).unwrap ();
		self.stdout.flush () ?;
		Ok (())
	}
	pub fn input (& mut self) -> GenResult <InputEvent> {
		match self.read () ? {
			by @ b'\x20' ..= b'\x7e' =>
				Ok (InputEvent::Key (Key::Char (by as char))),
			by @ b'\x01' ..= b'\x1a' =>
				Ok (InputEvent::CtrlKey (Key::Char ((by + b'a' - b'\x01') as char))),
			b'\x1b' => match self.read () ? {
				by @ b'\x01' ..= b'\x1a' =>
					Ok (InputEvent::CtrlAltKey (Key::Char ((by + b'a' - b'\x01') as char))),
				b'[' => {
					let mut vals = [0; 3];
					let mut vals_len = 0;
					let mut val = None;
					let err = |vals: [u32; 3], vals_len, by: u8|
						format! ("Invalid CSI: {:?} 0x{by:02x}", & vals [ .. vals_len]);
					loop {
						match self.read () ? {
							by @ b'0' ..= b'9' =>
								val = Some (val.unwrap_or (0_u32) * 10 + (by - b'0') as u32),
							b';' => {
								if vals_len == 2 { return Err ("Invalid CSI".into ()) }
								vals [vals_len] = val.ok_or ("Invalid CSI") ?;
								vals_len += 1;
								val = None;
							},
							by => {
								if let Some (val) = val {
									if vals_len == 3 { return Err ("Invalid CSI".into ()) }
									vals [vals_len] = val;
									vals_len += 1;
								}
								return Self::decode_csi (& vals [ .. vals_len], by)
									.ok_or_else (|| err (vals, vals_len, by).into ());
							},
						}
					}
				},
				by @ b'\x20' ..= b'\x7e' => Ok (InputEvent::AltKey (Key::Char (by as char))),
				by => Err (format! ("ESC + 0x{by:02x}").into ()),
			},
			b'\x7f' => Ok (InputEvent::Key (Key::Backspace)),
			by_0 @ b'\xc0' ..= b'\xdf' => {
				let err = |by_0, by_1| format! ("Invalid UTF-8: {by_0:02x} {by_1:02x}");
				match self.read () ? {
					by_1 @ b'\x80' ..= b'\xbf' => Ok (InputEvent::Key (Key::Char (char::from_u32 (
						((by_0 as u32 & 0x1f) << 6)
							| (by_1 as u32 & 0x3f)
					).ok_or_else (|| err (by_0, by_1)) ?))),
					by_1 => Err (err (by_0, by_1).into ()),
				}
			},
			by_0 @ b'\xe0' ..= b'\xef' => {
				let err = |by_0, by_1, by_2| format! ("Invalid UTF-8: {by_0:02x} {by_1:02x} {by_2:02x}");
				match (self.read () ?, self.read () ?) {
					(by_1 @ b'\x80' ..= b'\xbf', by_2 @ b'\x80' ..= b'\xbf') =>
						Ok (InputEvent::Key (Key::Char (char::from_u32 (
							((by_0 as u32 & 0x1f) << 12)
								| ((by_1 as u32 & 0x3f) << 6)
								| (by_2 as u32 & 0x3f)
						).ok_or_else (|| err (by_0, by_1, by_2)) ?))),
					(by_1, by_2) => Err (err (by_0, by_1, by_2).into ()),
				}
			},
			by => Err (format! ("Invalid input: {by:02x}").into ()),
		}
	}
	fn read (& mut self) -> GenResult <u8> {
		if self.buf_in_start == self.buf_in_end {
			self.buf_in_start = 0;
			self.buf_in_end = self.stdin.read (self.buf_in.as_mut_slice ()) ?;
		}
		let byte = self.buf_in [self.buf_in_start];
		self.buf_in_start += 1;
		Ok (byte)
	}
	pub fn move_to (& mut self, row: usize, col: usize) -> GenResult <()> {
		write! (self, "\x1b[{row};{col}H", row = row + 1, col = col + 1) ?;
		Ok (())
	}
	pub fn clear_to_end (& mut self) -> GenResult <()> {
		write! (self, "\x1b[K") ?;
		Ok (())
	}
	pub fn text_attr (& mut self, text_attr: TextAttr) -> GenResult <()> {
		write! (self, "\x1b[0m") ?;
		if text_attr.bold {
			write! (self, "\x1b[1m") ?;
		}
		if text_attr.italic {
			write! (self, "\x1b[3m") ?;
		}
		if text_attr.underline {
			write! (self, "\x1b[4m") ?;
		}
		write! (self,
			"\x1b[48;2;{red};{green};{blue}m",
			red = text_attr.bg.red,
			green = text_attr.bg.green,
			blue = text_attr.bg.blue) ?;
		write! (self,
			"\x1b[38;2;{red};{green};{blue}m",
			red = text_attr.fg.red,
			green = text_attr.fg.green,
			blue = text_attr.fg.blue) ?;
		Ok (())
	}
	fn decode_csi (vals: & [u32], by: u8) -> Option <InputEvent> {
		Some (match (
			vals,
			by,
		) {
			(& [8, rows, cols], b't') => InputEvent::TextSize { rows, cols },
			(& [9, rows, cols], b't') => InputEvent::ScreenSize { rows, cols },
			(& [], b'A') => InputEvent::Key (Key::Up),
			(& [1, 2], b'A') => InputEvent::ShiftKey (Key::Up),
			(& [1, 3], b'A') => InputEvent::AltKey (Key::Up),
			(& [1, 4], b'A') => InputEvent::AltShiftKey (Key::Up),
			(& [1, 5], b'A') => InputEvent::CtrlKey (Key::Up),
			(& [1, 6], b'A') => InputEvent::CtrlShiftKey (Key::Up),
			(& [], b'B') => InputEvent::Key (Key::Down),
			(& [1, 2], b'B') => InputEvent::ShiftKey (Key::Down),
			(& [1, 3], b'B') => InputEvent::AltKey (Key::Down),
			(& [1, 4], b'B') => InputEvent::AltShiftKey (Key::Down),
			(& [1, 5], b'B') => InputEvent::CtrlKey (Key::Down),
			(& [1, 6], b'B') => InputEvent::CtrlShiftKey (Key::Down),
			(& [], b'C') => InputEvent::Key (Key::Right),
			(& [1, 2], b'C') => InputEvent::ShiftKey (Key::Right),
			(& [1, 3], b'C') => InputEvent::AltKey (Key::Right),
			(& [1, 4], b'C') => InputEvent::AltShiftKey (Key::Right),
			(& [1, 5], b'C') => InputEvent::CtrlKey (Key::Right),
			(& [1, 6], b'C') => InputEvent::CtrlShiftKey (Key::Right),
			(& [], b'D') => InputEvent::Key (Key::Left),
			(& [1, 2], b'D') => InputEvent::ShiftKey (Key::Left),
			(& [1, 3], b'D') => InputEvent::AltKey (Key::Left),
			(& [1, 4], b'D') => InputEvent::AltShiftKey (Key::Left),
			(& [1, 5], b'D') => InputEvent::CtrlKey (Key::Left),
			(& [1, 6], b'D') => InputEvent::CtrlShiftKey (Key::Left),
			(& [], b'F') => InputEvent::Key (Key::End),
			(& [], b'H') => InputEvent::Key (Key::Home),
			(& [1, 2], b'P') => InputEvent::ShiftKey (Key::F1),
			(& [1, 5], b'P') => InputEvent::CtrlKey (Key::F1),
			(& [1, 6], b'P') => InputEvent::CtrlShiftKey (Key::F1),
			(& [1, 2], b'Q') => InputEvent::ShiftKey (Key::F2),
			(& [1, 5], b'Q') => InputEvent::CtrlKey (Key::F2),
			(& [1, 6], b'Q') => InputEvent::CtrlShiftKey (Key::F2),
			(& [1, 2], b'R') => InputEvent::ShiftKey (Key::F3),
			(& [1, 5], b'R') => InputEvent::CtrlKey (Key::F3),
			(& [1, 6], b'R') => InputEvent::CtrlShiftKey (Key::F3),
			(& [1, 2], b'S') => InputEvent::ShiftKey (Key::F4),
			(& [1, 5], b'S') => InputEvent::CtrlKey (Key::F4),
			(& [1, 6], b'S') => InputEvent::CtrlShiftKey (Key::F4),
			(& [], b'Z') => InputEvent::CtrlShiftKey (Key::Tab),
			(& [2], b'~') => InputEvent::Key (Key::Insert),
			(& [2, 3], b'~') => InputEvent::AltKey (Key::Insert),
			(& [3], b'~') => InputEvent::Key (Key::Delete),
			(& [3, 2], b'~') => InputEvent::ShiftKey (Key::Delete),
			(& [3, 3], b'~') => InputEvent::AltKey (Key::Delete),
			(& [3, 5], b'~') => InputEvent::CtrlKey (Key::Delete),
			(& [3, 6], b'~') => InputEvent::CtrlShiftKey (Key::Delete),
			(& [5], b'~') => InputEvent::Key (Key::PageUp),
			(& [6], b'~') => InputEvent::Key (Key::PageDown),
			(& [15], b'~') => InputEvent::Key (Key::F5),
			(& [15, 2, 2], b'~') => InputEvent::ShiftKey (Key::F5),
			(& [15, 5], b'~') => InputEvent::CtrlKey (Key::F5),
			(& [15, 6], b'~') => InputEvent::CtrlShiftKey (Key::F5),
			(& [17], b'~') => InputEvent::Key (Key::F6),
			(& [17, 2], b'~') => InputEvent::ShiftKey (Key::F6),
			(& [17, 5], b'~') => InputEvent::CtrlKey (Key::F6),
			(& [17, 6], b'~') => InputEvent::CtrlShiftKey (Key::F6),
			(& [18], b'~') => InputEvent::Key (Key::F7),
			(& [18, 2], b'~') => InputEvent::ShiftKey (Key::F7),
			(& [18, 5], b'~') => InputEvent::CtrlKey (Key::F7),
			(& [18, 6], b'~') => InputEvent::CtrlShiftKey (Key::F7),
			(& [19], b'~') => InputEvent::Key (Key::F8),
			(& [19, 2], b'~') => InputEvent::ShiftKey (Key::F8),
			(& [19, 3], b'~') => InputEvent::AltKey (Key::F8),
			(& [19, 5], b'~') => InputEvent::CtrlKey (Key::F8),
			(& [19, 6], b'~') => InputEvent::CtrlShiftKey (Key::F8),
			(& [20], b'~') => InputEvent::Key (Key::F9),
			(& [20, 2], b'~') => InputEvent::ShiftKey (Key::F9),
			(& [20, 3], b'~') => InputEvent::AltKey (Key::F9),
			(& [20, 4], b'~') => InputEvent::AltShiftKey (Key::F9),
			(& [20, 5], b'~') => InputEvent::CtrlKey (Key::F9),
			(& [20, 6], b'~') => InputEvent::CtrlShiftKey (Key::F9),
			(& [21, 5], b'~') => InputEvent::CtrlKey (Key::F10),
			(& [23, 2], b'~') => InputEvent::ShiftKey (Key::F11),
			(& [24], b'~') => InputEvent::Key (Key::F12),
			(& [24, 2], b'~') => InputEvent::ShiftKey (Key::F12),
			(& [24, 6], b'~') => InputEvent::CtrlShiftKey (Key::F12),
			_ => return None,
		})
	}
}

impl Drop for Terminal {
	fn drop (& mut self) {
		if self.termios.is_some () {
			self.stop ().unwrap ();
		}
	}
}

impl Write for Terminal {

	fn write (& mut self, bytes: & [u8]) -> io::Result <usize> {
		self.buf_out.extend_from_slice (bytes);
		Ok (bytes.len ())
	}

	fn flush (& mut self) -> io::Result <()> {
		let mut buf: & [u8] = & self.buf_out;
		while ! buf.is_empty () {
			let num_bytes = self.stdout.write (buf) ?;
			buf = & buf [num_bytes .. ];
		}
		self.buf_out.clear ();
		self.stdout.flush () ?;
		Ok (())
	}

}

#[ derive (Clone, Copy, Debug, Eq, PartialEq) ]
pub enum InputEvent {
	Key (Key),
	ShiftKey (Key),
	CtrlKey (Key),
	CtrlShiftKey (Key),
	CtrlAltKey (Key),
	AltKey (Key),
	AltShiftKey (Key),
	TextSize { rows: u32, cols: u32 },
	ScreenSize { rows: u32, cols: u32 },
}

#[ derive (Clone, Copy, Debug, Eq, PartialEq) ]
pub enum Key {
	Char (char),
	Tab, Insert, Delete, Backspace,
	Up, Down, Left, Right, PageUp, PageDown, Home, End,
	F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}

#[ derive (Clone, Copy) ]
pub struct TextAttr {
	pub fg: Colour,
	pub bg: Colour,
	pub bold: bool,
	pub underline: bool,
	pub italic: bool,
}

impl TextAttr {
	pub fn build (palette: & HashMap <Rc <str>, Colour>, src: & ConfigTextAttr) -> GenResult <Self> {
		Ok (Self {
			fg: * palette.get (& src.fg).ok_or_else (|| format! ("Missing colour: {}", src.fg)) ?,
			bg: * palette.get (& src.bg).ok_or_else (|| format! ("Missing colour: {}", src.bg)) ?,
			bold: src.bold,
			underline: false,
			italic: false,
		})
	}
}
