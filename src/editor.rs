use std::cmp;
use std::io::Write as _;
use std::iter;

use crate::buffer::*;
use crate::config::*;
use crate::misc::*;
use crate::terminal::{ InputEvent as InEv, Key, Terminal };

pub struct Editor {
	term: Terminal,
	term_rows: usize,
	term_cols: usize,
	kill_buf: Vec <Vec <char>>,
	prev_event: Option <InEv>,
	buffer: Buffer,
	config: Config,
	error: Option <String>,
}

impl Editor {
	pub fn new () -> GenResult <Self> {
		let mut term = Terminal::new () ?;
		term.start () ?;
		Ok (Self {
			term,
			term_rows: 25,
			term_cols: 80,
			kill_buf: Vec::new (),
			prev_event: None,
			buffer: Buffer::new ("Cargo.toml".to_owned ()) ?,
			config: Config {
				tab_size: 4,
			},
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
				InEv::AltKey (Key::Char ('x')) => break,
				InEv::Key (Key::Char (ch)) => self.buffer.insert_char (ch),
				InEv::Key (Key::Up) | InEv::CtrlKey (Key::Char ('p')) => self.buffer.up (1),
				InEv::Key (Key::Down) | InEv::CtrlKey (Key::Char ('n')) => self.buffer.down (1),
				InEv::Key (Key::Left) | InEv::CtrlKey (Key::Char ('b')) => self.buffer.left (1),
				InEv::Key (Key::Right) | InEv::CtrlKey (Key::Char ('f')) => self.buffer.right (1),
				InEv::Key (Key::PageUp) | InEv::AltKey (Key::Char ('v')) => self.buffer.up (self.term_rows - 4),
				InEv::Key (Key::PageDown) | InEv::CtrlKey (Key::Char ('v')) => self.buffer.down (self.term_rows - 4),
				InEv::Key (Key::Backspace) => self.buffer.backspace (),
				InEv::Key (Key::Delete) | InEv::CtrlKey (Key::Char ('d')) => self.buffer.delete (),
				InEv::Key (Key::Home) | InEv::CtrlKey (Key::Char ('a')) => self.buffer.home (),
				InEv::Key (Key::End) | InEv::CtrlKey (Key::Char ('e')) => self.buffer.end (),
				InEv::CtrlKey (Key::Char ('i')) => self.buffer.insert_char ('\t'),
				InEv::CtrlKey (Key::Char ('k')) => {
					if self.prev_event != Some (InEv::CtrlKey (Key::Char ('k'))) {
						self.kill_buf = Vec::new ();
					}
					self.buffer.kill (& mut self.kill_buf);
				},
				InEv::CtrlKey (Key::Char ('l')) => {
					write! (self.term, "\x1b[?1049h") ?;
					write! (self.term, "\x1b[18t") ?;
				},
				InEv::CtrlKey (Key::Char ('m')) => self.buffer.enter (),
				InEv::CtrlKey (Key::Char ('s')) => self.buffer.save () ?,
				InEv::CtrlKey (Key::Char ('u')) => self.buffer.unkill (& self.kill_buf),
				InEv::CtrlKey (Key::Char ('z')) => unsafe {
					let pid = libc::getpid ();
					self.term.stop () ?;
					libc::kill (pid, libc::SIGSTOP);
					self.term.start () ?;
				},
				ev => new_error = Some (format! ("EVENT: {ev:?}").into ()),
			}
			self.error = new_error;
			self.prev_event = Some (ev);
			self.draw () ?;
		}
		Ok (())
	}
	fn draw (& mut self) -> GenResult <()> {
		self.term.move_to (0, 0) ?;
		write! (self.term, "== {name}{dirty} ==",
			name = self.buffer.name (),
			dirty = if self.buffer.dirty () { " *" } else { "" }) ?;
		self.term.clear_to_end () ?;
		self.term.move_to (self.term_rows - 1, 0) ?;
		if let Some (error) = self.error.as_ref () {
			write! (self.term, "== ERROR: {error} ==") ?;
		} else {
			write! (self.term, "== {status} ==",
				status = self.buffer.status ()) ?;
		}
		self.term.clear_to_end () ?;
		self.buffer.draw (& mut self.term, 1, self.term_rows - 1) ?;
		Ok (())
	}
}
