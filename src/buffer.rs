use std::fs::File;
use std::io::{ self, Read as _, Write as _ };
use std::iter;

use crate::misc::*;
use crate::terminal::*;

pub struct Buffer {
	filename: String,
	lines: Vec <Vec <char>>,
	dirty: bool,
	line_idx: usize,
	char_idx: usize,
	col_idx: usize,
	saved_col_idx: usize,
	tab_size: usize,
	auto_indent: bool,
}

impl Buffer {
	pub fn new (filename: String) -> GenResult <Self> {
		let mut file = match File::open (& filename) {
			Ok (file) => file,
			Err (err) if err.kind () == io::ErrorKind::NotFound => {
				return Ok (Self {
					filename,
					lines: vec! [ Vec::new () ],
					dirty: true,
					line_idx: 0,
					char_idx: 0,
					col_idx: 0,
					saved_col_idx: 0,
					tab_size: 4,
					auto_indent: true,
				});
			},
			Err (err) => return Err (err.into ()),
		};
		let mut buf = [0_u8; 4096];
		let mut buf_start = 0;
		let mut buf_end = 0;
		let mut lines = Vec::new ();
		let mut line = Vec::new ();
		loop {
			if buf_end - buf_start < 4 {
				let mut buf_temp = [0_u8; 3];
				let mut buf_temp_len = 0;
				while buf_start < buf_end {
					buf_temp [buf_temp_len] = buf [buf_start];
					buf_start += 1;
					buf_temp_len += 1;
				}
				buf_start = 0;
				buf_end = file.read (& mut buf [0 .. 4096 - buf_temp_len]) ?;
				for & by in & buf_temp [ .. buf_temp_len] {
					buf [buf_end] = by;
					buf_end += 1;
				}
			}
			if buf_start == buf_end { break }
			let mut next = {
				let buf = & mut buf;
				let buf_start = & mut buf_start;
				|| if * buf_start == buf_end { None } else {
					let by = buf [* buf_start];
					* buf_start += 1;
					Some (by)
				}
			};
			match next () {
				Some (b'\n') => { lines.push (line); line = Vec::new (); },
				Some (by_0 @ 0x00 ..= 0x7f) => line.push (by_0 as char),
				Some (by_0 @ 0xc0 ..= 0xdf) => match next () {
					Some (by_1 @ 0x80 ..= 0xbf) => line.push (char::from_u32 (
						((by_0 as u32) & 0x1f) << 6 | ((by_1 as u32) & 0x3f)
					).ok_or ("Invalid UTF-8") ?),
					_ => return Err ("Invalid UTF-8".into ()),
				},
				_ => todo! (),
				None => break,
			}
		}
		lines.push (line);
		Ok (Self {
			filename,
			lines,
			dirty: false,
			line_idx: 0,
			char_idx: 0,
			col_idx: 0,
			saved_col_idx: 0,
			tab_size: 4,
			auto_indent: true,
		})
	}
	pub fn save (& mut self) -> GenResult <()> {
		let mut file = File::create (& self.filename) ?;
		let mut first = true;
		for line in & self.lines {
			if ! first {
				write! (file, "\n") ?;
			} else {
				first = false;
			}
			for & ch in line {
				write! (file, "{}", ch) ?;
			}
		}
		file.flush () ?;
		self.dirty = false;
		Ok (())
	}
	pub fn name (& self) -> & str {
		& self.filename
	}
	pub fn dirty (& self) -> bool {
		self.dirty
	}
	pub fn insert_char (& mut self, ch: char) {
		let char_idx = self.char_idx;
		self.line_mut ().insert (char_idx, ch);
		self.set_char_idx (self.char_idx + 1);
		self.dirty = true;
	}
	pub fn enter (& mut self) {
		let char_idx = self.char_idx;
		let line = self.line_mut ().split_off (char_idx);
		self.lines.insert (self.line_idx + 1, line);
		self.set_line_idx (self.line_idx + 1);
		self.set_char_idx (0);
		let indent: Vec <char> =
			self.lines [self.line_idx - 1].iter ().copied ()
				.take_while (|& ch| ch == ' ' || ch == '\t')
				.collect ();
		for ch in indent { self.insert_char (ch); }
		self.dirty = true;
	}
	pub fn up (& mut self, num: usize) {
		for _ in 0 .. num {
			if self.line_idx == 0 { return }
			self.set_line_idx (self.line_idx - 1);
		}
	}
	pub fn down (& mut self, num: usize) {
		for _ in 0 .. num {
			if self.lines.len () <= self.line_idx + 1 { return }
			self.set_line_idx (self.line_idx + 1);
		}
	}
	pub fn left (& mut self, num: usize) {
		for _ in 0 .. num {
			if 0 < self.char_idx {
				self.set_char_idx (self.char_idx - 1);
			} else if 0 < self.line_idx {
				self.set_line_idx (self.line_idx - 1);
				self.set_char_idx (self.lines [self.line_idx].len ());
			} else {
				return;
			}
		}
	}
	pub fn right (& mut self, num: usize) {
		for _ in 0 .. num {
			let line = & self.lines [self.line_idx];
			if self.char_idx < line.len () {
				self.set_char_idx (self.char_idx + 1);
			} else if self.line_idx + 1 < self.lines.len () {
				self.set_line_idx (self.line_idx + 1);
				self.set_char_idx (0);
			} else {
				return;
			}
		}
	}
	pub fn home (& mut self) {
		self.set_char_idx (0);
	}
	pub fn end (& mut self) {
		self.set_char_idx (self.line ().len ());
	}
	pub fn delete (& mut self) {
		let line = & mut self.lines [self.line_idx];
		if self.char_idx < line.len () {
			line.remove (self.char_idx);
		} else if self.line_idx + 1 < self.lines.len () {
			let mut line_1 = self.lines.remove (self.line_idx + 1);
			let line_0 = & mut self.lines [self.line_idx];
			line_0.append (& mut line_1);
		}
		self.dirty = true;
	}
	pub fn backspace (& mut self) {
		if 0 < self.char_idx {
			self.set_char_idx (self.char_idx - 1);
			let char_idx = self.char_idx;
			self.line_mut ().remove (char_idx);
		} else if 0 < self.line_idx {
			let mut temp = self.lines.remove (self.line_idx);
			self.set_line_idx (self.line_idx - 1);
			self.set_char_idx (self.line ().len ());
			self.line_mut ().append (& mut temp);
		}
		self.dirty = true;
	}
	pub fn kill (& mut self, kill_buf: & mut Vec <Vec <char>>) {
		if self.line_idx + 1 < self.lines.len () {
			let temp = self.lines.remove (self.line_idx);
			kill_buf.push (temp);
		} else if ! self.line ().is_empty () {
			let temp = self.lines.remove (self.line_idx);
			kill_buf.push (temp);
			self.lines.push (Vec::new ());
		}
		self.set_line_idx (self.line_idx);
		self.dirty = true;
	}
	pub fn unkill (& mut self, kill_buf: & [Vec <char>]) {
		let char_idx = self.char_idx;
		let mut temp = self.line_mut ().split_off (char_idx);
		for kill_idx in 0 .. kill_buf.len () {
			let kill_line = & kill_buf [kill_idx];
			self.lines [self.line_idx].extend_from_slice (kill_line);
			self.lines.insert (self.line_idx + 1, Vec::new ());
			self.set_line_idx (self.line_idx + 1);
		}
		self.set_char_idx (0);
		self.line_mut ().append (& mut temp);
		self.dirty = true;
	}
	fn set_line_idx (& mut self, line_idx: usize) {
		self.line_idx = line_idx;
		self.char_idx = 0;
		self.col_idx = 0;
		for & ch in & self.lines [self.line_idx] {
			let next_col_idx = if ch == '\t' {
				self.col_idx - self.col_idx % self.tab_size + self.tab_size
			} else {
				self.col_idx + 1
			};
			if self.saved_col_idx < next_col_idx { break }
			self.col_idx = next_col_idx;
			self.char_idx += 1;
		}
	}
	fn set_char_idx (& mut self, char_idx: usize) {
		self.char_idx = char_idx;
		self.col_idx = 0;
		for & ch in self.lines [self.line_idx].iter ().take (char_idx) {
			self.col_idx = if ch == '\t' {
				self.col_idx - self.col_idx % self.tab_size + self.tab_size
			} else {
				self.col_idx + 1
			};
		}
		self.saved_col_idx = self.col_idx;
	}
	fn line (& self) -> & [char] {
		 & self.lines [self.line_idx]
	}
	fn line_mut (& mut self) -> & mut Vec <char> {
		 & mut self.lines [self.line_idx]
	}
	pub fn draw (& self, term: & mut Terminal, start: usize, end: usize) -> GenResult <()> {
		let line_num_len = (self.lines.len () + 1).to_string ().len ();
		let mut buf = String::new ();
		for ((row_idx, line_idx), line) in (start .. end)
				.zip (0 .. )
				.zip (self.lines.iter ().map (Some).chain (iter::repeat (None))) {
			buf.clear ();
			term.move_to (row_idx, 0) ?;
			let mut col = 0;
			if let Some (line) = line {
				write! (term, "{line_num:line_num_len$} ", line_num = line_idx + 1) ?;
				for & ch in line {
					if ch == '\t' {
						let num = self.tab_size - col % self.tab_size;
						for _ in 0 .. num {
							write! (term, " ") ?;
							col += 1;
						}
					} else {
						write! (term, "{ch}") ?;
						col += 1;
					}
				}
			}
			term.clear_to_end () ?
		}
		term.move_to (start + self.line_idx, line_num_len + 1 + self.col_idx) ?;
		term.flush () ?;
		Ok (())
	}
	pub fn status (& self) -> String {
		format! (
			"line {line}/{lines}, col {col}/{cols}",
			line = self.line_idx + 1,
			lines = self.lines.len (),
			col = self.col_idx + 1,
			cols = "TODO")
	}
}
