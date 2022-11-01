use std::error::Error;
use std::io::{ self, Read as _, Stdin, Stdout, Write };
use std::os::unix::io::AsRawFd as _;
use std::process::ExitCode;
use termios::Termios;

use crate::misc::*;

pub struct Terminal {
	stdin: Stdin,
	stdout: Stdout,
	termios: Option <Termios>,
	buffer: Vec <u8>,
	buffer_start: usize,
	buffer_end: usize,
}

impl Terminal {
	pub fn new () -> GenResult <Terminal> {
		Ok (Self {
			stdin: io::stdin (),
			stdout: io::stdout (),
			termios: None,
			buffer: vec! [0_u8; 1024],
			buffer_start: 0,
			buffer_end: 0,
		})
	}
	pub fn start (& mut self) -> GenResult <()> {
		assert! (self.termios.is_none ());
		let mut termios = Termios::from_fd (self.stdin.as_raw_fd ()) ?;
		self.termios = Some (termios);
		termios::cfmakeraw (& mut termios);
		termios::tcsetattr (self.stdin.as_raw_fd (), termios::TCSANOW, & termios) ?;
		write! (self.stdout, "\x1b[?1049h") ?;
		Ok (())
	}
	pub fn stop (& mut self) -> GenResult <()> {
		assert! (self.termios.is_some ());
		write! (self, "\x1b[?1049l").unwrap ();
		self.flush ().unwrap ();
		let termios = self.termios.take ().unwrap ();
		termios::tcsetattr (self.stdin.as_raw_fd (), termios::TCSANOW, & termios).unwrap ();
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
		if self.buffer_start == self.buffer_end {
			self.buffer_start = 0;
			self.buffer_end = self.stdin.read (self.buffer.as_mut_slice ()) ?;
		}
		let byte = self.buffer [self.buffer_start];
		self.buffer_start += 1;
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
	fn decode_csi (vals: & [u32], by: u8) -> Option <InputEvent> {
		Some (match (
			vals.len (),
			vals.get (0).copied ().unwrap_or (0),
			vals.get (1).copied ().unwrap_or (0),
			vals.get (2).copied ().unwrap_or (0),
			by,
		) {
			(3, 8, rows, cols, b't') => InputEvent::TextSize { rows, cols },
			(3, 9, rows, cols, b't') => InputEvent::ScreenSize { rows, cols },
			(0, _, _, _, b'A') => InputEvent::Key (Key::Up),
			(2, 1, 2, _, b'A') => InputEvent::ShiftKey (Key::Up),
			(2, 1, 3, _, b'A') => InputEvent::AltKey (Key::Up),
			(2, 1, 4, _, b'A') => InputEvent::AltShiftKey (Key::Up),
			(2, 1, 5, _, b'A') => InputEvent::CtrlKey (Key::Up),
			(2, 1, 6, _, b'A') => InputEvent::CtrlShiftKey (Key::Up),
			(0, _, _, _, b'B') => InputEvent::Key (Key::Down),
			(2, 1, 2, _, b'B') => InputEvent::ShiftKey (Key::Down),
			(2, 1, 3, _, b'B') => InputEvent::AltKey (Key::Down),
			(2, 1, 4, _, b'B') => InputEvent::AltShiftKey (Key::Down),
			(2, 1, 5, _, b'B') => InputEvent::CtrlKey (Key::Down),
			(2, 1, 6, _, b'B') => InputEvent::CtrlShiftKey (Key::Down),
			(0, _, _, _, b'C') => InputEvent::Key (Key::Right),
			(2, 1, 2, _, b'C') => InputEvent::ShiftKey (Key::Right),
			(2, 1, 3, _, b'C') => InputEvent::AltKey (Key::Right),
			(2, 1, 4, _, b'C') => InputEvent::AltShiftKey (Key::Right),
			(2, 1, 5, _, b'C') => InputEvent::CtrlKey (Key::Right),
			(2, 1, 6, _, b'C') => InputEvent::CtrlShiftKey (Key::Right),
			(0, _, _, _, b'D') => InputEvent::Key (Key::Left),
			(2, 1, 2, _, b'D') => InputEvent::ShiftKey (Key::Left),
			(2, 1, 3, _, b'D') => InputEvent::AltKey (Key::Left),
			(2, 1, 4, _, b'D') => InputEvent::AltShiftKey (Key::Left),
			(2, 1, 5, _, b'D') => InputEvent::CtrlKey (Key::Left),
			(2, 1, 6, _, b'D') => InputEvent::CtrlShiftKey (Key::Left),
			(0, _, _, _, b'F') => InputEvent::Key (Key::End),
			(0, _, _, _, b'H') => InputEvent::Key (Key::Home),
			(2, 1, 2, _, b'P') => InputEvent::ShiftKey (Key::F1),
			(2, 1, 5, _, b'P') => InputEvent::CtrlKey (Key::F1),
			(2, 1, 6, _, b'P') => InputEvent::CtrlShiftKey (Key::F1),
			(2, 1, 2, _, b'Q') => InputEvent::ShiftKey (Key::F2),
			(2, 1, 5, _, b'Q') => InputEvent::CtrlKey (Key::F2),
			(2, 1, 6, _, b'Q') => InputEvent::CtrlShiftKey (Key::F2),
			(2, 1, 2, _, b'R') => InputEvent::ShiftKey (Key::F3),
			(2, 1, 5, _, b'R') => InputEvent::CtrlKey (Key::F3),
			(2, 1, 6, _, b'R') => InputEvent::CtrlShiftKey (Key::F3),
			(2, 1, 2, _, b'S') => InputEvent::ShiftKey (Key::F4),
			(2, 1, 5, _, b'S') => InputEvent::CtrlKey (Key::F4),
			(2, 1, 6, _, b'S') => InputEvent::CtrlShiftKey (Key::F4),
			(1, 2, _, _, b'~') => InputEvent::Key (Key::Insert),
			(2, 2, 3, _, b'~') => InputEvent::AltKey (Key::Insert),
			(1, 3, _, _, b'~') => InputEvent::Key (Key::Delete),
			(2, 3, 2, _, b'~') => InputEvent::ShiftKey (Key::Delete),
			(2, 3, 3, _, b'~') => InputEvent::AltKey (Key::Delete),
			(2, 3, 5, _, b'~') => InputEvent::CtrlKey (Key::Delete),
			(2, 3, 6, _, b'~') => InputEvent::CtrlShiftKey (Key::Delete),
			(1, 5, _, _, b'~') => InputEvent::Key (Key::PageUp),
			(1, 6, _, _, b'~') => InputEvent::Key (Key::PageDown),
			(1, 15, _, _, b'~') => InputEvent::Key (Key::F5),
			(2, 15, 2, 2, b'~') => InputEvent::ShiftKey (Key::F5),
			(2, 15, 5, _, b'~') => InputEvent::CtrlKey (Key::F5),
			(2, 15, 6, _, b'~') => InputEvent::CtrlShiftKey (Key::F5),
			(1, 17, _, _, b'~') => InputEvent::Key (Key::F6),
			(2, 17, 2, _, b'~') => InputEvent::ShiftKey (Key::F6),
			(2, 17, 5, _, b'~') => InputEvent::CtrlKey (Key::F6),
			(2, 17, 6, _, b'~') => InputEvent::CtrlShiftKey (Key::F6),
			(1, 18, _, _, b'~') => InputEvent::Key (Key::F7),
			(2, 18, 2, _, b'~') => InputEvent::ShiftKey (Key::F7),
			(2, 18, 5, _, b'~') => InputEvent::CtrlKey (Key::F7),
			(2, 18, 6, _, b'~') => InputEvent::CtrlShiftKey (Key::F7),
			(1, 19, _, _, b'~') => InputEvent::Key (Key::F8),
			(2, 19, 2, _, b'~') => InputEvent::ShiftKey (Key::F8),
			(2, 19, 3, _, b'~') => InputEvent::AltKey (Key::F8),
			(2, 19, 5, _, b'~') => InputEvent::CtrlKey (Key::F8),
			(2, 19, 6, _, b'~') => InputEvent::CtrlShiftKey (Key::F8),
			(1, 20, _, _, b'~') => InputEvent::Key (Key::F9),
			(2, 20, 2, _, b'~') => InputEvent::ShiftKey (Key::F9),
			(2, 20, 3, _, b'~') => InputEvent::AltKey (Key::F9),
			(2, 20, 4, _, b'~') => InputEvent::AltShiftKey (Key::F9),
			(2, 20, 5, _, b'~') => InputEvent::CtrlKey (Key::F9),
			(2, 20, 6, _, b'~') => InputEvent::CtrlShiftKey (Key::F9),
			(2, 21, 5, _, b'~') => InputEvent::CtrlKey (Key::F10),
			(2, 23, 2, _, b'~') => InputEvent::ShiftKey (Key::F11),
			(1, 24, _, _, b'~') => InputEvent::Key (Key::F12),
			(2, 24, 2, _, b'~') => InputEvent::ShiftKey (Key::F12),
			(2, 24, 6, _, b'~') => InputEvent::CtrlShiftKey (Key::F12),
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
		self.stdout.write (bytes)
	}
	fn flush (& mut self) -> io::Result <()> {
		self.stdout.flush ()
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
	Insert, Delete, Backspace,
	Up, Down, Left, Right, PageUp, PageDown, Home, End,
	F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}
