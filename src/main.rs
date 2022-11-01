use std::process::ExitCode;

mod config;
mod buffer;
mod editor;
mod terminal;

use crate::editor::Editor;
use crate::misc::*;

mod misc {
	use std::error::Error;
	pub type GenError = Box <dyn Error>;
	pub type GenResult <Val> = Result <Val, GenError>;
}

fn main () -> GenResult <ExitCode> {
	let mut editor = Editor::new () ?;
	editor.run () ?;
	Ok (ExitCode::SUCCESS)
}
