use std::env;
use std::panic;
use std::process::ExitCode;

mod config;
mod buffer;
mod editor;
mod file;
mod line;
mod misc;
mod terminal;

use crate::editor::*;
use crate::file::*;
use crate::misc::*;

fn main () -> ExitCode {
	match panic::catch_unwind (|| -> GenResult <()> {
		let files: Vec <File> =
			env::args ().skip (1)
				.map (|filename| File::load (filename.into ()))
				.collect::<GenResult <_>> () ?;
		let mut editor = Editor::new (files) ?;
		editor.run () ?;
		Ok (())
	}) {
		Ok (Ok (())) => ExitCode::SUCCESS,
		Ok (Err (err)) => {
			eprintln! ("Error: {err}");
			ExitCode::FAILURE
		},
		Err (panic) => {
			match panic.downcast::<String> () {
				Ok (panic) => eprintln! ("Panic: {panic}"),
				Err (panic) => match panic.downcast::<& str> () {
					Ok (panic) => eprintln! ("Panic: {panic}"),
					Err (panic) => eprintln! ("Panic: {:?}", panic.type_id ()),
				},
			}
			ExitCode::FAILURE
		},
	}
}
