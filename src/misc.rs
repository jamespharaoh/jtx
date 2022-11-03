use std::error::Error;

pub type GenError = Box <dyn Error>;
pub type GenResult <Val> = Result <Val, GenError>;

#[ macro_export ]
macro_rules! some_or {
	($expr:expr, $else:expr) => {
		match $expr {
			Some (val) => val,
			None => $else,
		}
	}
}
