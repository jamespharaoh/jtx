use std::mem;
use std::ops::Deref;
use std::rc::Rc;

use crate::line::*;

pub struct Buffer {
	lines: Vec <Line>,
	line_idx: usize,
	char_idx: usize,
}

impl Buffer {

	pub fn line_idx (& self) -> usize {
		self.line_idx
	}

	pub fn char_idx (& self) -> usize {
		self.char_idx
	}

	pub fn num_lines (& self) -> usize {
		self.lines.len ()
	}

	pub fn move_to (& mut self, line_idx: usize, char_idx: usize) {
		debug_assert! (line_idx < self.lines.len ());
		debug_assert! (char_idx <= self.lines [line_idx].len ());
		debug_assert! (self.lines [line_idx].is_char_boundary (char_idx));
		self.line_idx = line_idx;
		self.char_idx = char_idx;
	}

	pub fn move_left (& mut self, mut num: usize) {
		while self.char_idx < num {
			if self.line_idx == 0 {
				self.char_idx = 0;
				return;
			}
			num -= self.line_left ().chars ().count () + 1;
			self.line_idx -= 1;
			self.char_idx = self.line ().len ();
		}
		self.char_idx -=
			self.line_left ().chars ().rev ()
				.take (num)
				.map (char::len_utf8)
				.sum::<usize> ();
	}

	pub fn move_right (& mut self, mut num: usize) {
		while self.line ().len () < self.char_idx + num {
			if self.line_idx + 1 == self.lines.len () {
				self.char_idx = self.line ().len ();
				return;
			}
			num -= self.line_right ().chars ().count () + 1;
			self.line_idx += 1;
			self.char_idx = 0;
		}
		self.char_idx +=
			self.line_right ().chars ()
				.take (num)
				.map (char::len_utf8)
				.sum::<usize> ();
	}

	pub fn insert_char (& mut self, ch: char) {
		if ch == '\n' {
			let char_idx = self.char_idx;
			let line_0 = Line::Owned (self.line () [ .. char_idx].to_owned ());
			let line_1 = Line::Owned (self.line () [char_idx .. ].to_owned ());
			self.lines.splice (self.line_idx .. self.line_idx + 1, [ line_0, line_1 ]);
			self.line_idx += 1;
			self.char_idx = 0;
		} else {
			let char_idx = self.char_idx;
			self.line_mut ().insert (char_idx, ch);
			self.char_idx += ch.len_utf8 ();
		}
	}

	pub fn insert_str (& mut self, src: & str) {
		for ch in src.chars () {
			self.insert_char (ch);
		}
	}

	pub fn line (& self) -> & str {
		 self.lines [self.line_idx].as_str ()
	}

	pub fn line_left (& self) -> & str {
		& self.lines [self.line_idx].as_str () [ .. self.char_idx]
	}

	pub fn line_right (& self) -> & str {
		& self.lines [self.line_idx].as_str () [self.char_idx .. ]
	}

	fn line_mut (& mut self) -> & mut String {
		 self.lines [self.line_idx].make_mut ()
	}

	pub fn delete_char_left (& mut self) -> Option <char> {
		if 0 < self.char_idx {
			let ch = self.line_left ().chars ().next_back ().unwrap ();
			self.char_idx -= ch.len_utf8 ();
			let char_idx = self.char_idx;
			Some (self.line_mut ().remove (char_idx))
		} else if 0 < self.line_idx {
			self.line_idx -= 1;
			self.char_idx = self.line ().len ();
			let line = self.lines.remove (self.line_idx + 1);
			self.line_mut ().push_str (& line);
			Some ('\n')
		} else {
			None
		}
	}

	pub fn delete_char_right (& mut self) -> Option <char> {
		if self.char_idx < self.line ().len () {
			let char_idx = self.char_idx;
			Some (self.line_mut ().remove (char_idx))
		} else if self.line_idx + 1 < self.lines.len () {
			let line = self.lines.remove (self.line_idx + 1);
			self.line_mut ().push_str (& line);
			Some ('\n')
		} else {
			None
		}
	}

	pub fn cut_line (& mut self) -> String {
		if self.line_idx + 1 < self.lines.len () {
			let mut result = self.lines.remove (self.line_idx).to_owned ();
			self.char_idx = 0;
			result.push ('\n');
			result
		} else {
			mem::replace (self.line_mut (), String::new ())
		}
	}

	pub fn cut_bytes_right (& mut self, num_bytes: usize) -> String {

		// handle single line

		let rest = & self.line_right ();
		if num_bytes <= rest.len () {
			let result = rest [ .. num_bytes].to_string ();
			let char_idx = self.char_idx;
			self.line_mut ().replace_range (char_idx .. char_idx + num_bytes, "");
			return result;
		}

		// handle multi-line

		let mut result = String::with_capacity (num_bytes);
		result.push_str (rest);
		result.push ('\n');
		let mut rem_bytes = num_bytes - rest.len () - 1;
		let mut line_idx = self.line_idx + 1;
		while self.lines [line_idx].len () < rem_bytes {
			result.push_str (self.lines [line_idx].as_str ());
			result.push ('\n');
			rem_bytes -= self.lines [line_idx].len () + 1;
			line_idx += 1;
		}
		result.push_str (& self.lines [line_idx] [ .. rem_bytes]);
		let temp = self.lines [line_idx] [rem_bytes .. ].to_string ();
		let char_idx = self.char_idx;
		self.line_mut ().truncate (char_idx);
		self.line_mut ().push_str (& temp);
		self.lines.splice (self.line_idx + 1 .. line_idx + 1, []);
		debug_assert_eq! (num_bytes, result.len ());

		result

	}

	pub fn peek_left (& self) -> Option <char> {
		self.line_left ().chars ().next_back ()
			.or ((0 < self.line_idx).then_some ('\n'))
	}

	pub fn peek_right (& self) -> Option <char> {
		self.line_right ().chars ().next ()
			.or ((self.line_idx < self.lines.len ()).then_some ('\n'))
	}

}

impl Default for Buffer {

	fn default () -> Self {
		let lines = vec! [ Line::Owned ("".to_owned ()) ];
		Self { lines, line_idx: 0, char_idx: 0 }
	}

}

impl Deref for Buffer {

	type Target = [Line];

	fn deref (& self) -> & [Line] {
		& self.lines
	}

}

impl From <& Rc <String>> for Buffer {

	fn from (src: & Rc <String>) -> Self {
		let mut prev = 0;
		let mut lines = Vec::new ();
		while let Some (next) = src [prev .. ].find ('\n') {
			let next = prev + next;
			lines.push (Line::Shared (src.clone (), prev, next));
			prev = next + 1;
		}
		lines.push (Line::Shared (src.clone (), prev, src.len ()));
		Self { lines, line_idx: 0, char_idx: 0 }
	}

}
