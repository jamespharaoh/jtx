use std::cell::RefCell;
use std::fs::File as FsFile;
use std::io::{ self, Read as _, Write as _ };
use std::iter;
use std::rc::Rc;

use crate::*;
use crate::buffer::*;
use crate::misc::*;
use crate::terminal::*;

#[ derive (Clone, Debug) ]
enum Action {
	Insert {
		line_idx: usize,
		char_idx: usize,
		data: String,
	},
	Delete {
		line_idx: usize,
		char_idx: usize,
		num_bytes: usize,
	},
}

#[ derive (Clone, Copy, Debug) ]
enum Activity {
	None,
	Typing,
	Deleting,
	Backspacing,
	Killing,
}

pub struct File {
	state: Rc <RefCell <FileState>>,
}

pub struct FileState {
	filename: Rc <str>,
	buffer: Buffer,
	dirty: bool,
	col_idx: usize,
	saved_col_idx: usize,
	tab_size: usize,
	auto_indent: bool,
	undo: Vec <Action>,
	redo: Vec <Action>,
	activity: Activity,
	line_offset: usize,
}

impl File {

	pub fn new (filename: Rc <str>, buffer: Buffer, dirty: bool) -> Self {
		Self {
			state: Rc::new (RefCell::new (FileState {
				filename,
				buffer,
				dirty,
				col_idx: 0,
				saved_col_idx: 0,
				tab_size: 4,
				auto_indent: true,
				undo: Vec::new (),
				redo: Vec::new (),
				activity: Activity::None,
				line_offset: 0,
			})),
		}
	}

	pub fn load (filename: Rc <str>) -> GenResult <Self> {
		let mut file = match FsFile::open (& * filename) {
			Ok (file) => file,
			Err (err) if err.kind () == io::ErrorKind::NotFound => {
				return Ok (Self::new (filename, Buffer::default (), true));
			},
			Err (err) => return Err (err.into ()),
		};
		let mut data = String::new ();
		file.read_to_string (& mut data) ?;
		let data = Rc::new (data);
		Ok (Self::new (filename, (& data).into (), false))
	}

	pub fn save (& self) -> GenResult <()> {
		let mut state = self.state.borrow_mut ();
		let mut file = FsFile::create (& * state.filename) ?;
		let mut first = true;
		for line in & * state.buffer {
			if ! first {
				write! (file, "\n") ?;
			} else {
				first = false;
			}
			write! (file, "{}", & ** line) ?;
		}
		file.flush () ?;
		state.dirty = false;
		state.activity = Activity::None;
		Ok (())
	}

	pub fn name (& self) -> Rc <str> {
		Rc::clone (& self.state.borrow ().filename)
	}

	pub fn dirty (& self) -> bool {
		self.state.borrow ().dirty
	}

	pub fn type_char (& self, ch: char) {
		let mut state = self.state.borrow_mut ();
		if let (Activity::Typing, Some (& mut Action::Delete { ref mut num_bytes, .. })) =
				(state.activity, state.undo.last_mut ()) {
			* num_bytes += ch.len_utf8 ();
		} else {
			let action = Action::Delete {
				line_idx: state.buffer.line_idx (),
				char_idx: state.buffer.char_idx (),
				num_bytes: ch.len_utf8 (),
			};
			state.undo.push (action);
			state.redo.clear ();
		}
		state.buffer.insert_char (ch);
		if ch == '\n' && state.auto_indent {
			let indent: Vec <char> =
				state.buffer [state.buffer.line_idx () - 1].chars ()
					.take_while (|& ch| ch == ' ' || ch == '\t')
					.collect ();
			for ch in indent {
				state.buffer.insert_char (ch);
			}
		}
		state.fix_col_idx ();
		state.activity = if ch != '\n' { Activity::Typing } else { Activity::None };
		state.dirty = true;
	}

	pub fn undo (& self) {
		let mut state = self.state.borrow_mut ();
		if let Some (action) = state.undo.pop () {
			let action = state.perform (action);
			state.redo.push (action);
		}
		state.activity = Activity::None;
	}

	pub fn redo (& self) {
		let mut state = self.state.borrow_mut ();
		if let Some (action) = state.redo.pop () {
			let action = state.perform (action);
			state.undo.push (action);
		}
		state.activity = Activity::None;
	}

	pub fn up (& self, num: usize) {
		let mut state = self.state.borrow_mut ();
		state.activity = Activity::None;
		if num < state.buffer.line_idx () {
			let line_idx = state.buffer.line_idx () - num;
			state.set_line_idx (line_idx);
		} else {
			state.set_line_idx (0);
		}
	}

	pub fn down (& self, num: usize) {
		let mut state = self.state.borrow_mut ();
		state.activity = Activity::None;
		if state.buffer.line_idx () + num < state.buffer.num_lines () {
			let line_idx = state.buffer.line_idx () + num;
			state.set_line_idx (line_idx);
		} else {
			let line_idx = state.buffer.num_lines () - 1;
			state.set_line_idx (line_idx);
		}
	}

	pub fn left (& self, num: usize) {
		let mut state = self.state.borrow_mut ();
		state.activity = Activity::None;
		state.buffer.move_left (num);
		state.fix_col_idx ();
	}

	pub fn right (& self, num: usize) {
		let mut state = self.state.borrow_mut ();
		state.activity = Activity::None;
		state.buffer.move_right (num);
		state.fix_col_idx ();
	}

	pub fn home (& self) {
		let mut state = self.state.borrow_mut ();
		state.activity = Activity::None;
		let line_idx = state.buffer.line_idx ();
		state.buffer.move_to (line_idx, 0);
		state.fix_col_idx ();
	}

	pub fn end (& self) {
		let mut state = self.state.borrow_mut ();
		state.activity = Activity::None;
		let line_idx = state.buffer.line_idx ();
		let char_idx = state.buffer.line ().len ();
		state.buffer.move_to (line_idx, char_idx);
		state.fix_col_idx ();
	}

	pub fn delete (& self) {
		let mut state = self.state.borrow_mut ();
		let ch = some_or! (state.buffer.delete_char_right (), return);
		if ch != '\n' && matches! (state.activity, Activity::Deleting) {
			if let Some (& mut Action::Insert { ref mut data, .. }) = state.undo.last_mut () {
				data.push (ch);
			} else { unreachable! () }
		} else {
			let action = Action::Insert {
				line_idx: state.buffer.line_idx (),
				char_idx: state.buffer.char_idx (),
				data: ch.to_string (),
			};
			state.undo.push (action);
		}
		state.redo.clear ();
		state.activity = Activity::Deleting;
		state.dirty = true;
	}

	pub fn backspace (& self) {
		let mut state = self.state.borrow_mut ();
		let ch = some_or! (state.buffer.delete_char_left (), return);
		state.fix_col_idx ();
		if ch != '\n' && matches! (state.activity, Activity::Backspacing) {
			let buf_char_idx = state.buffer.char_idx ();
			if let Some (& mut Action::Insert { ref mut char_idx, ref mut data, .. }) =
					state.undo.last_mut () {
				* char_idx = buf_char_idx;
				data.insert (0, ch);
			} else { unreachable! () }
		} else {
			let action = Action::Insert {
				line_idx: state.buffer.line_idx (),
				char_idx: state.buffer.char_idx (),
				data: ch.to_string (),
			};
			state.undo.push (action);
		}
		state.redo.clear ();
		state.activity = Activity::Backspacing;
		state.dirty = true;
	}

	pub fn kill (& self, kill_buf: & mut String) {
		let mut state = self.state.borrow_mut ();
		let temp = state.buffer.cut_line ();
		kill_buf.push_str (& temp);
		state.fix_col_idx ();
		state.dirty = true;
		if matches! (state.activity, Activity::Killing) {
			if let Some (Action::Insert { ref mut data, .. }) = state.undo.last_mut () {
				data.push_str (& temp);
			} else { unreachable! () }
		} else {
			let action = Action::Insert {
				line_idx: state.buffer.line_idx (),
				char_idx: 0,
				data: temp,
			};
			state.undo.push (action);
		}
		state.redo.clear ();
		state.activity = Activity::Killing;
	}

	pub fn unkill (& self, kill_buf: & str) {
		let mut state = self.state.borrow_mut ();
		let action = Action::Delete {
			line_idx: state.buffer.line_idx (),
			char_idx: state.buffer.char_idx (),
			num_bytes: kill_buf.len (),
		};
		state.undo.push (action);
		state.buffer.insert_str (kill_buf);
		state.fix_col_idx ();
		state.activity = Activity::None;
	}

	pub fn draw (
		& self,
		term: & mut Terminal,
		ui_attrs: & UiAttrs,
		start: usize,
		end: usize,
	) -> GenResult <()> {
		let mut state = self.state.borrow_mut ();
		let line_num_len = (state.buffer.num_lines () + 1).to_string ().len ();
		let mut buf = String::new ();
		if state.buffer.line_idx () < state.line_offset {
			state.line_offset = state.buffer.line_idx ();
		}
		if state.line_offset + end - start - 1 < state.buffer.line_idx () {
			state.line_offset = state.buffer.line_idx () - (end - start - 1);
		}
		for ((row_idx, line_idx), line) in (start .. end)
				.zip (state.line_offset .. )
				.zip (state.buffer [state.line_offset .. ].iter ()
					.map (Some)
					.chain (iter::repeat (None))) {
			buf.clear ();
			term.move_to (row_idx, 0) ?;
			let mut col = 0;
			if let Some (line) = line {
				term.text_attr (ui_attrs.line_nums) ?;
				write! (term, "{line_num:line_num_len$} ", line_num = line_idx + 1) ?;
				term.text_attr (ui_attrs.default) ?;
				for ch in line.chars () {
					if ch == '\t' {
						let num = state.tab_size - col % state.tab_size;
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
		term.move_to (
			start + state.buffer.line_idx () - state.line_offset,
			line_num_len + 1 + state.col_idx) ?;
		term.flush () ?;
		Ok (())
	}

	pub fn status (& self) -> String {
		let state = self.state.borrow ();
		format! (
			"line {line}/{lines}  col {col}/{cols}",
			line = state.buffer.line_idx () + 1,
			lines = state.buffer.num_lines (),
			col = state.col_idx + 1,
			cols = 1 + state.buffer.line ().chars ()
				.fold (0, |cols, ch| cols + if ch == '\t' {
					state.tab_size - cols % state.tab_size
				} else { 1 }))
	}

}

impl FileState {

	fn perform (& mut self, action: Action) -> Action {
		self.dirty = true;
		match action {
			Action::Delete { line_idx, char_idx, num_bytes } => {
				self.buffer.move_to (line_idx, char_idx);
				self.fix_col_idx ();
				let data = self.buffer.cut_bytes_right (num_bytes);
				Action::Insert { line_idx, char_idx, data }
			},
			Action::Insert { line_idx, char_idx, data } => {
				self.buffer.move_to (line_idx, char_idx);
				let action = Action::Delete { line_idx, char_idx, num_bytes: data.len () };
				self.buffer.insert_str (& data);
				self.fix_col_idx ();
				action
			},
		}
	}
	
	fn set_line_idx (& mut self, line_idx: usize) {
		let mut char_idx = 0;
		self.col_idx = 0;
		for ch in self.buffer [line_idx].chars () {
			let next_col_idx = if ch == '\t' {
				self.col_idx - self.col_idx % self.tab_size + self.tab_size
			} else {
				self.col_idx + 1
			};
			if self.saved_col_idx < next_col_idx { break }
			self.col_idx = next_col_idx;
			char_idx += 1;
		}
		self.buffer.move_to (line_idx, char_idx);
	}

	fn fix_col_idx (& mut self) {
		self.col_idx = 0;
		for ch in self.buffer.line_left ().chars () {
			self.col_idx = if ch == '\t' {
				self.col_idx - self.col_idx % self.tab_size + self.tab_size
			} else {
				self.col_idx + 1
			};
		}
		self.saved_col_idx = self.col_idx;
	}

}
