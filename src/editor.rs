use std::collections::HashMap;
use std::io::Write as _;

use crate::config::*;
use crate::file::*;
use crate::misc::*;
use crate::terminal::{ TextAttr, InputEvent as InEv, Key, Terminal };

pub struct Editor {
	term: Terminal,
	term_rows: usize,
	term_cols: usize,
	kill_buf: String,
	prev_event: Option <InEv>,
	files: Vec <File>,
	file_idx: usize,
	config: Config,
	ui_attrs: UiAttrs,
	error: Option <String>,
}

impl Editor {

	pub fn new (files: Vec <File>) -> GenResult <Self> {
		let mut term = Terminal::new () ?;
		term.start () ?;
		let config = Config::load () ?;
		let ui_attrs = UiAttrs::build (& config) ?;
		Ok (Self {
			term,
			term_rows: 25,
			term_cols: 80,
			kill_buf: String::new (),
			prev_event: None,
			files,
			file_idx: 0,
			config,
			ui_attrs,
			error: None,
		})
	}

	pub fn run (& mut self) -> GenResult <()> {
		write! (self.term, "\x1b[18t") ?;
		self.term.flush () ?;
		loop {
			let ev = match self.term.input () {
				Ok (ev) => ev,
				Err (err) => {
					self.error = Some (err.to_string ());
					self.draw () ?;
					continue;
				},
			};
			let mut new_error = None;
			match ev {
				InEv::TextSize { rows, cols } => {
					self.term_rows = rows as usize;
					self.term_cols = cols as usize;
				},
				InEv::Key (Key::Char (ch)) => self.file ().type_char (ch),
				InEv::Key (Key::Up) | InEv::CtrlKey (Key::Char ('p')) => self.file ().up (1),
				InEv::Key (Key::Down) | InEv::CtrlKey (Key::Char ('n')) => self.file ().down (1),
				InEv::Key (Key::Left) | InEv::CtrlKey (Key::Char ('b')) => self.file ().left (1),
				InEv::Key (Key::Right) | InEv::CtrlKey (Key::Char ('f')) => self.file ().right (1),
				InEv::Key (Key::PageUp) | InEv::AltKey (Key::Char ('v')) => self.file ().up (self.term_rows - 4),
				InEv::Key (Key::PageDown) | InEv::CtrlKey (Key::Char ('v')) => self.file ().down (self.term_rows - 4),
				InEv::Key (Key::Backspace) => self.file ().backspace (),
				InEv::Key (Key::Delete) | InEv::CtrlKey (Key::Char ('d')) => self.file ().delete (),
				InEv::Key (Key::Home) | InEv::CtrlKey (Key::Char ('a')) => self.file ().home (),
				InEv::Key (Key::End) | InEv::CtrlKey (Key::Char ('e')) => self.file ().end (),
				InEv::CtrlKey (Key::Char ('i')) => self.file ().type_char ('\t'),
				InEv::CtrlKey (Key::Char ('k')) => {
					if self.prev_event != Some (InEv::CtrlKey (Key::Char ('k'))) {
						self.kill_buf = String::new ();
					}
					self.files [self.file_idx].kill (& mut self.kill_buf);
				},
				InEv::CtrlKey (Key::Char ('l')) => {
					write! (self.term, "\x1b[?1049h") ?;
					write! (self.term, "\x1b[18t") ?;
				},
				InEv::CtrlKey (Key::Char ('m')) => self.file ().type_char ('\n'),
				InEv::CtrlKey (Key::Char ('s')) => self.file ().save () ?,
				InEv::CtrlKey (Key::Char ('u')) => self.file ().unkill (& self.kill_buf),
				InEv::CtrlKey (Key::Char ('z')) => unsafe {
					let pid = libc::getpid ();
					self.term.stop () ?;
					libc::kill (pid, libc::SIGSTOP);
					self.term.start () ?;
				},
				InEv::AltKey (Key::Char ('e')) => self.file ().redo (),
				InEv::AltKey (Key::Char ('u')) => self.file ().undo (),
				InEv::AltKey (Key::Char ('x')) => break,
				InEv::AltKey (Key::Left) => {
					self.file_idx =
						if self.file_idx == 0 { self.files.len () - 1 }
						else { self.file_idx - 1 };
				},
				InEv::AltKey (Key::Right) => {
					self.file_idx += 1;
					if self.file_idx == self.files.len () { self.file_idx = 0; }
				},
				ev => new_error = Some (format! ("EVENT: {ev:?}").into ()),
			}
			self.error = new_error;
			self.prev_event = Some (ev);
			self.draw () ?;
		}
		Ok (())
	}

	fn file (& self) -> & File {
		& self.files [self.file_idx]
	}

	fn draw (& mut self) -> GenResult <()> {
		self.term.move_to (0, 0) ?;
		self.term.text_attr (self.ui_attrs.header) ?;
		write! (self.term, "  [{file_idx}/{file_count}]  {name}{dirty}",
			file_idx = self.file_idx + 1,
			file_count = self.files.len (),
			name = self.files [self.file_idx].name (),
			dirty = if self.files [self.file_idx].dirty () { " *" } else { "" }) ?;
		self.term.clear_to_end () ?;
		self.term.move_to (self.term_rows - 1, 0) ?;
		self.term.text_attr (self.ui_attrs.status) ?;
		if let Some (error) = self.error.as_ref () {
			write! (self.term, "  ERROR: {error}") ?;
		} else {
			write! (self.term, "  {status}",
				status = self.file ().status ()) ?;
		}
		self.term.clear_to_end () ?;
		self.files [self.file_idx].draw (
			& mut self.term,
			& self.ui_attrs,
			2,
			self.term_rows - 2) ?;
		self.term.flush () ?;
		Ok (())
	}

}

pub struct UiAttrs {
	pub default: TextAttr,
	pub header: TextAttr,
	pub status: TextAttr,
	pub line_nums: TextAttr,
}

impl UiAttrs {

	fn build (config: & Config) -> GenResult <Self> {
		Ok (Self {
			default: TextAttr::build (& config.palette, & config.ui.default) ?,
			header: TextAttr::build (& config.palette, & config.ui.header) ?,
			status: TextAttr::build (& config.palette, & config.ui.status) ?,
			line_nums: TextAttr::build (& config.palette, & config.ui.line_nums) ?,
		})
	}

}
