use std::ops::Deref;
use std::rc::Rc;

pub enum Line {
	Owned (String),
	Shared (Rc <String>, usize, usize),
}

impl Line {

	pub fn as_str (& self) -> & str {
		match self {
			& Self::Owned (ref val) => val,
			& Self::Shared (ref val, start, end) => & val [start .. end],
		}
	}

	pub fn make_mut (& mut self) -> & mut String {
		match self {
			& mut Self::Owned (ref mut val) => val,
			& mut Self::Shared (ref val, start, end) => {
				* self = Self::Owned (val [start .. end].to_owned ());
				if let & mut Self::Owned (ref mut val) = self { val } else { unreachable! () }
			},
		}
	}

}

impl Deref for Line {
	type Target = str;
	fn deref (& self) -> & str {
		self.as_str ()
	}
}

impl From <& str> for Line {
	fn from (src: & str) -> Self {
		Self::Owned (src.to_owned ())
	}
}
