use crate::util;
use serde::{Deserialize, Serialize};

/// Identifies a global index or thread feed
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum FeedID {
	Index,
	Catalog,
	Thread {
		/// Thread ID
		id: u64,

		/// Page currently navigated to
		page: i32,
	},
}

impl Default for FeedID {
	#[inline]
	fn default() -> Self {
		Self::Index
	}
}

impl FeedID {
	/// Return corresponding integer feed code used by the server
	#[inline]
	pub fn as_u64(&self) -> u64 {
		use FeedID::*;

		match self {
			Index | Catalog => 0,
			Thread { id, .. } => *id,
		}
	}
}

/// Post or page margin to scroll to
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub enum Focus {
	Top,
	Bottom,
	Post(u64),
}

impl Default for Focus {
	#[inline]
	fn default() -> Focus {
		Focus::Top
	}
}

/// Location the app can navigate to
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug, Default)]
pub struct Location {
	pub feed: FeedID,

	/// Focus a post after a successful render
	pub focus: Option<Focus>,
}

impl Location {
	pub fn from_path() -> Location {
		let loc = util::window().location();
		let path = loc.pathname().unwrap_or_default();
		let split: Vec<&str> = path.split('/').collect();
		Location {
			feed: match (split.get(1), split.len()) {
				(Some(&"threads"), 4) => {
					macro_rules! parse {
						($i:expr) => {
							split.get($i).map(|s| s.parse().ok()).flatten()
						};
					}

					match (parse!(2), parse!(3)) {
						(Some(thread), Some(page)) => FeedID::Thread {
							id: thread,
							page: page,
						},
						_ => FeedID::Index,
					}
				}
				(Some(&"catalog"), _) => FeedID::Catalog,
				_ => FeedID::Index,
			},
			focus: loc
				.hash()
				.ok()
				.map(|h| match h.as_str() {
					"#top" => Some(Focus::Top),
					"#bottom" => Some(Focus::Bottom),
					_ => {
						if h.starts_with("#p-") {
							h[3..].parse().ok().map(|id| Focus::Post(id))
						} else {
							None
						}
					}
				})
				.flatten(),
		}
	}

	pub fn path(&self) -> String {
		use FeedID::*;
		use Focus::*;

		let mut w: String = match &self.feed {
			Index => "/".into(),
			Catalog => "/catalog".into(),
			Thread { id, page } => format!("/threads/{}/{}", id, page),
		};
		if let Some(f) = &self.focus {
			match f {
				Bottom => {
					w += "#bottom";
				}
				Top => {
					w += "#top";
				}
				Post(id) => {
					use std::fmt::Write;

					write!(w, "#p-{}", id).unwrap();
				}
			}
		}
		w
	}

	/// Returns, if this is a single thread page
	#[inline]
	pub fn is_thread(&self) -> bool {
		matches!(self.feed, FeedID::Thread { .. })
	}
}
