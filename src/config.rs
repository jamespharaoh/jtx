use serde::Deserialize;
use serde::de;
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::rc::Rc;

use crate::misc::*;

#[ derive (Deserialize) ]
pub struct Config {
	pub misc: ConfigMisc,
	pub palette: HashMap <Rc <str>, Colour>,
	pub ui: ConfigUi,
	//pub colour_schemes: HashMap <Rc <str>, Rc <str>>,
	//pub file_types_by_extension: HashMap <Rc <str>, Rc <FileType>>,
}

impl Config {
	pub fn load () -> GenResult <Self> {
		let config_path = format! (
			"{home}/.config/jtx/config",
			home = env::var ("HOME").unwrap ());
		let mut config_str = fs::read_to_string (config_path) ?;
		Ok (toml::from_str (& config_str) ?)
	}
}

#[ derive (Deserialize) ]
pub struct ConfigUi {
	pub default: ConfigTextAttr,
	pub header: ConfigTextAttr,
	pub status: ConfigTextAttr,
	#[ serde (rename = "line-nums") ]
	pub line_nums: ConfigTextAttr,
}

#[ derive (Deserialize) ]
pub struct ConfigMisc {
	#[ serde (rename = "tab-size") ]
	pub tab_size: usize,
}

pub struct FileType {
	pub name: Rc <str>,
}

#[ derive (Deserialize) ]
pub struct ConfigTextAttr {
	pub fg: Rc <str>,
	pub bg: Rc <str>,
	#[ serde (default) ]
	pub bold: bool,
}

#[ derive (Clone, Copy) ]
pub struct Colour {
	pub red: u8,
	pub green: u8,
	pub blue: u8,
}

impl <'de> Deserialize <'de> for Colour {
	fn deserialize <De: de::Deserializer <'de>> (de: De) -> Result <Self, De::Error> {
		struct Visitor;
		impl <'de> de::Visitor <'de> for Visitor {
			type Value = Colour;
			fn expecting (& self, fmtr: & mut fmt::Formatter) -> fmt::Result {
				fmtr.write_str ("Colour")
			}
			fn visit_str <Er: de::Error> (self, src: & str) -> Result <Colour, Er> {
				if src.chars ().count () != 7
						|| src.chars ().next ().unwrap () != '#'
						|| ! src.chars ().skip (1).all (|ch| ch.is_ascii_hexdigit ()) {
					return Err (de::Error::invalid_value (
						de::Unexpected::Str (src),
						& "'#' and six hex digits"));
				}
				Ok (Colour {
					red: u8::from_str_radix (& src [1 .. 3], 16).unwrap (),
					green: u8::from_str_radix (& src [3 .. 5], 16).unwrap (),
					blue: u8::from_str_radix (& src [5 .. 7], 16).unwrap (),
				})
			}
		}
		de.deserialize_str (Visitor)
	}
}
