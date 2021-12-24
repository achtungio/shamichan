use super::{state, FeedID, Focus, Location, State};
use crate::{connection::send, mouse::Coordinates, util};
use common::{
	payloads::{Post, Thread, ThreadWithPosts},
	util::BidirectionalSetMap,
	MessageType,
};
use indexmap::IndexSet;
use std::collections::{hash_map::Entry, HashMap};
use wasm_bindgen::JsCast;
use yew::{
	agent::{AgentLink, Bridge, Context, Dispatched, HandlerId},
	Callback, Component, ComponentLink,
};
use yew_services::render::{RenderService, RenderTask};

// TODO: resync on disconnect
// TODO: received page trigger
// TODO: request new pages to be fetched on current thread

// TODO: differentiate between updates coming from the index and thread feeds
// to prevent duplicate messages. This can be done via a boolean on all post
// update messages (implement a trait that sets a boolean for these messages to
// be called inside write_post_message).

/// Push new location to history
const PUSH_STATE: u8 = 1;

/// Set new location to global state and trigger updates
const SET_STATE: u8 = 1 << 1;

/// Scroll to to the set location, if anything focused
const SCROLL_TO_FOCUSED: u8 = 1 << 2;

/// Do not trigger updates on new location setting
const NO_TRIGGER: u8 = 1 << 3;

/// Subscribe to updates of a value type
pub enum Request {
	NotifyChange(Vec<Change>),

	/// Change the current notifications a client is subscribed to
	ChangeNotifications {
		remove: Vec<Change>,
		add: Vec<Change>,
	},

	/// Fetch feed data
	FetchFeed(Location),

	/// Navigate to the app to a different feed
	NavigateTo {
		loc: Location,
		flags: u8,
	},

	/// Set or delete the ID of the currently used KeyPair
	SetKeyID(Option<uuid::Uuid>),

	/// Set post as created by this user
	SetMine(u64),

	/// Set ID of currently open post
	SetOpenPostID(Option<u64>),

	/// Insert a new post into the registry
	RegisterPost(Post),

	/// Register a page's posts in the application state
	RegisterPage(Vec<Post>),

	/// Register thread metainformation
	RegisterThreadMeta(Thread),

	/// Register a single thread and its posts
	RegisterThread(ThreadWithPosts),

	/// Register threads passed from the thread index feed
	RegisterThreads(Vec<ThreadWithPosts>),

	/// Apply a patch to an existing post body
	PatchPostBody(common::payloads::post_body::PostBodyPatch),

	/// Close an open body
	ClosePost(common::payloads::post_body::PostBody),

	/// Set tags used on threads
	SetUsedTags(Vec<String>),

	/// Set time correction between the server and client
	SetTimeCorrection(i32),

	/// Set configs received from the server
	SetConfigs(common::config::Public),

	/// Mark a post as pinned or not pinned to the screen and set it's
	// coordinates
	SetPostPinCoords {
		/// Post ID
		post: u64,

		/// Change to apply to a post's pin status
		change: PostPinChange,
	},
}

/// Selective changes of global state to be notified on
#[derive(Eq, PartialEq, Hash, Copy, Clone, Debug)]
pub enum Change {
	/// Change of location the app is navigated to
	Location,

	/// Authentication key pair has been set by user
	KeyPair,

	/// Change to any field of Options
	Options,

	/// Change to any field of the Configs
	Configs,

	/// Change in tags used on threads
	UsedTags,

	/// Subscribe to changes of the list of threads
	ThreadList,

	/// Subscribe to thread data changes, excluding the post content level.
	/// Includes changes to the post set of threads.
	Thread(u64),

	/// Subscribe to any changes to a post
	Post(u64),

	/// Change in time correction value
	TimeCorrection,

	/// Change of the open allocated post ID
	OpenPostID,
}

/// Change to apply to a post's pin status
#[derive(Debug)]
pub enum PostPinChange {
	/// Set coordinates of post
	Set(Coordinates),

	/// Increment the current value of the coordinates
	Increment(Coordinates),

	/// Unmark the post as pinned
	Remove,
}

/// Abstraction over AgentLink and ComponentLink
pub trait Link {
	type Message;

	fn make_callback<F>(&self, f: F) -> Callback<()>
	where
		F: Fn(()) -> Self::Message + 'static;
}

impl<A: yew::agent::Agent> Link for AgentLink<A> {
	type Message = A::Message;

	#[inline]
	fn make_callback<F>(&self, f: F) -> Callback<()>
	where
		F: Fn(()) -> Self::Message + 'static,
	{
		self.callback(f)
	}
}

impl<C: Component> Link for ComponentLink<C> {
	type Message = C::Message;

	#[inline]
	fn make_callback<F>(&self, f: F) -> Callback<()>
	where
		F: Fn(()) -> Self::Message + 'static,
	{
		self.callback(f)
	}
}

/// Helper for storing a hook into state updates and read-only access to the
/// app state in the client struct
pub struct StateBridge {
	bridge: Box<dyn Bridge<Agent>>,
}

impl StateBridge {
	/// Send a message to the state app agent
	#[inline]
	pub fn send(&mut self, req: Request) {
		self.bridge.send(req);
	}

	/// Returns an immutable reference to the app state
	#[inline]
	pub fn get(&self) -> std::cell::Ref<'static, State> {
		state::get_ref()
	}
}

/// Create hooks into state changes and gain read-only access to reading the
/// app state
pub fn hook<L, F>(link: &L, changes: Vec<Change>, f: F) -> StateBridge
where
	L: Link,
	F: Fn() -> L::Message + 'static,
{
	use yew::agent::Bridged;

	let mut b = StateBridge {
		bridge: Agent::bridge(link.make_callback(move |_| f())),
	};
	if !changes.is_empty() {
		b.bridge.send(Request::NotifyChange(changes));
	}
	b
}

pub enum Message {
	ScrollTo(Focus),
	PoppedState,
}

/// Feed synchronization state
#[derive(Debug)]
enum FeedSyncState {
	/// No feed requested yet
	NotRequested,

	/// Feed requested but not all requested data received
	Receiving {
		/// Location being synced to
		loc: Location,

		/// Thread metainformation received from the server
		thread: Option<Thread>,

		/// Thread pages that need to be received or are already
		pages: HashMap<i32, Option<Vec<Post>>>,

		/// Flags passed during the fetch
		flags: u8,
	},

	/// Fully synced to server feed
	Synced {
		/// Feed ID
		feed: FeedID,

		/// Thread pages that need to be received or are already.
		/// `true` means it has been received.
		pages: HashMap<u32, bool>,
	},
}

// /// Arguments used for merging a feed from websocket and JSON API data
// struct FeedMergeArgs {
// 	loc: Location,
// 	flags: u8,
// 	from_json: Vec<ThreadWithPosts>,
// 	from_websocket: HashMap<u64, FeedData>,
// }

/// Global state storage and propagation agent
pub struct Agent {
	link: AgentLink<Self>,

	/// Clients hooked into change notifications
	hooks: BidirectionalSetMap<Change, HandlerId>,

	/// Change notifications pending flushing to clients.
	queued_triggers: IndexSet<HandlerId>,

	/// Task used to defer actions to the next animation frame
	render_task: Option<RenderTask>,

	/// State of synchronization to the current or pending feed
	feed_sync_state: FeedSyncState,
}

impl yew::agent::Agent for Agent {
	type Reach = Context<Self>;
	type Message = Message;
	type Input = Request;
	type Output = ();

	#[cold]
	fn create(link: AgentLink<Self>) -> Self {
		util::add_static_listener(
			util::window(),
			"popstate",
			true,
			link.callback(|_: web_sys::Event| Message::PoppedState),
		);

		Self {
			link,
			hooks: BidirectionalSetMap::default(),
			render_task: None,
			feed_sync_state: FeedSyncState::NotRequested,
			queued_triggers: Default::default(),
		}
	}

	fn update(&mut self, msg: Self::Message) {
		use Message::*;

		match msg {
			ScrollTo(f) => {
				Self::scroll_to(&f);
			}
			PoppedState => self.set_location(
				Location::from_path(),
				SET_STATE | SCROLL_TO_FOCUSED,
			),
		}

		self.flush_triggers();
	}

	fn handle_input(&mut self, req: Self::Input, id: HandlerId) {
		use Request::*;

		match req {
			NotifyChange(h) => {
				for h in h {
					self.hooks.insert(h, id);
				}
			}
			ChangeNotifications { remove, add } => {
				for h in remove {
					self.hooks.remove_by_key_value(&h, &id);
				}
				for h in add {
					self.hooks.insert(h, id);
				}
			}
			NavigateTo { loc, flags } => self.set_location(loc, flags),
			FetchFeed(loc) => {
				self.try_sync_feed(&loc, SCROLL_TO_FOCUSED);
			}
			SetKeyID(id) => util::with_logging(|| {
				let mut s = state::get_mut();
				s.key_pair.id = id;
				s.key_pair.store()
			}),
			RegisterPost(p) => {
				self.trigger(&Change::Thread(p.thread));
				self.trigger(&Change::Post(p.id));
				state::get_mut().register_post(p);
			}
			PatchPostBody(msg) => {
				if let Some(p) = state::get_mut().posts.get_mut(&msg.id) {
					util::with_logging(|| {
						let mut new = (*p.body).clone();
						new.patch(msg.patch)?;
						p.body = new.into();
						self.trigger(&Change::Post(msg.id));
						Ok(())
					});
				}
			}
			ClosePost(msg) => {
				let mut s = state::get_mut();
				if let Some(p) = s.posts.get_mut(&msg.id) {
					p.body = msg.body;
					p.open = false;
					self.trigger(&Change::Post(msg.id));
				}
				if s.open_post_id == Some(msg.id) {
					s.open_post_id = None;
					self.trigger(&Change::OpenPostID);
				}
			}
			RegisterPage(posts) => self.register_page(posts),
			RegisterThreads(threads) => self.register_threads(threads),
			RegisterThread(thread) => {
				self.register_thread(&mut *state::get_mut(), thread);
			}
			RegisterThreadMeta(thread) => match &mut self.feed_sync_state {
				FeedSyncState::Receiving {
					loc, thread: dst, ..
				} if loc.feed.as_u64() == thread.id => {
					*dst = Some(thread);
				}
				_ => (),
			},
			SetMine(id) => {
				// TODO: persist to DB
				state::get_mut().mine.insert(id);
			}
			SetOpenPostID(id) => {
				state::get_mut().open_post_id = id;
				self.trigger(&Change::OpenPostID);
			}
			SetUsedTags(tags) => {
				state::get_mut().used_tags = tags.into();
				self.trigger(&Change::UsedTags);
			}
			SetTimeCorrection(c) => {
				state::get_mut().time_correction = c;
				self.trigger(&Change::TimeCorrection);
			}
			SetConfigs(c) => {
				state::get_mut().configs = c;
				self.trigger(&Change::Configs);
			}
			SetPostPinCoords { post, change } => {
				match (change, state::get_mut().pinned_posts.entry(post)) {
					(PostPinChange::Set(c), Entry::Vacant(e)) => {
						e.insert(c);
					}
					(PostPinChange::Set(c), Entry::Occupied(mut e)) => {
						*e.get_mut() = c;
					}
					(PostPinChange::Increment(c), Entry::Occupied(mut e)) => {
						*e.get_mut() += c;
					}
					(PostPinChange::Remove, Entry::Occupied(e)) => {
						e.remove();
					}
					_ => (),
				};
				self.trigger(&Change::Post(post));
			}
		};

		self.flush_triggers();
	}

	fn disconnected(&mut self, id: HandlerId) {
		self.hooks.remove_by_value(&id);
	}
}

impl Agent {
	/// Schedule to send change notification to hooked clients.
	///
	/// Triggers need to be flushed to send the notifications.
	///
	/// Trigger these updates in hierarchical order to make any upper level
	/// components switch rendering modes and not cause needless updates
	/// on deleted child components.
	///
	/// Notifications are buffered to reduce double notification chances and any
	/// overhead of double sending and double handling.
	fn trigger(&mut self, h: &Change) {
		if let Some(subs) = self.hooks.get_by_key(h) {
			for id in subs.iter() {
				self.queued_triggers.insert(*id);
			}
		}
	}

	/// Flush queued notifications to clients
	fn flush_triggers(&mut self) {
		for id in self.queued_triggers.drain(0..) {
			self.link.respond(id, ());
		}
	}

	/// Set app location and propagate changes
	fn set_location(&mut self, new: Location, flags: u8) {
		// TODO: when navigating between the pages of the same thread,
		// intelligently preserve scroll position, if the new page is adjacent
		// to the old one from - both before and after. Needs to account for
		// locking to bottom.

		let mut s = state::get_mut();
		let old = s.location.clone();
		if old == new {
			// Scroll to focused element, even if the location did not change.
			// This enables automatic scrolling to the focused element even
			// after some other (possibly user-caused) scrolling ocurred.
			if flags & SCROLL_TO_FOCUSED != 0 {
				if let Some(f) = &new.focus {
					Self::scroll_to(f);
				}
			}
			return;
		}

		log::debug!(
			"set_location: {:?} -> {:?}, flags={}",
			s.location,
			new,
			flags
		);

		let mut try_to_sync = true;

		// Check, if feed did not change, only requesting a new page
		match (&mut self.feed_sync_state, &new.feed) {
			(
				FeedSyncState::Synced { feed, pages },
				FeedID::Thread { id, page: new_page },
			) if &feed.as_u64() == id => {
				let mut new_page = *new_page;
				if new_page < -1 {
					util::log_and_alert_error(
						&"requested negative page ID smaller than -1",
					);
					return;
				}
				if new_page < 0 {
					new_page += s.get_synced_thread(id).page_count as i32;
				}
				if let Entry::Vacant(e) = pages.entry(new_page as u32) {
					e.insert(false);
					send(MessageType::Page, &new_page);
					try_to_sync = false;
				}
			}
			_ => (),
		};

		if try_to_sync && self.try_sync_feed(&new, flags) {
			return;
		}

		self.set_location_no_sync(&mut *s, new, flags);
	}

	/// Set app location and propagate changes without trying to sync the feed
	/// first, if needed
	fn set_location_no_sync(
		&mut self,
		s: &mut super::State,
		new: Location,
		flags: u8,
	) {
		log::debug!(
			"set_location_no_sync: {:?} -> {:?}, flags={}",
			s.location,
			new,
			flags
		);

		if flags & SET_STATE != 0 {
			s.location = new.clone();
			if flags & NO_TRIGGER == 0 {
				self.trigger(&Change::Location);
			}
		}
		if flags & SCROLL_TO_FOCUSED != 0 {
			if let Some(f) = new.focus.clone() {
				self.render_task = RenderService::request_animation_frame(
					self.link.callback(move |_| Message::ScrollTo(f.clone())),
				)
				.into();
			}
		}

		if flags & PUSH_STATE != 0 {
			util::with_logging(|| {
				util::window().history()?.push_state_with_url(
					&wasm_bindgen::JsValue::NULL,
					"",
					Some(&new.path()),
				)?;
				Ok(())
			});
		}
	}

	/// Scroll the browser to the focused element, if any
	fn scroll_to(f: &Focus) {
		use self::Focus::*;
		use util::document;
		use web_sys::HtmlElement;

		let y = match f {
			Top => 0.0,
			Bottom => document()
				.document_element()
				.map(|el| el.scroll_height())
				.unwrap_or_default() as f64,
			Post(id) => document()
				.get_element_by_id(&format!("p-{}", id))
				.map(|el| {
					el.dyn_into::<HtmlElement>().ok().map(|el| {
						el.get_bounding_client_rect().top() as f64
							+ util::window().scroll_y().unwrap()
							- document()
								.get_element_by_id("banner")
								.map(|el| {
									el.dyn_into::<HtmlElement>().ok().map(
										|el| {
											// Add an extra 5px offset for some
											// margin between the focused
											// element and the banner
											el.offset_height() + 5
										},
									)
								})
								.flatten()
								.unwrap_or_default() as f64
					})
				})
				.flatten()
				.unwrap_or_default(),
		};
		log::debug!("scrolling to: {:?} = (0, {})", f, y);
		util::window().scroll_with_x_and_y(0.0, y);
	}

	/// Register posts of a page in the application state
	fn register_page(&mut self, posts: Vec<Post>) {
		use FeedSyncState::*;

		let (thread_id, page) = match posts.first() {
			Some(p) => (p.thread, p.page),
			None => return,
		};

		// Import threads only once we know the import is valid, to not
		// overwrite data from different feeds

		match &mut self.feed_sync_state {
			Receiving {
				loc,
				pages,
				flags,
				thread,
			} if thread.is_some() && loc.feed.as_u64() == thread_id => {
				pages.insert(page as i32, Some(posts));

				// Also insert the page number counted from the back to prevent
				// duplicate requests
				let page_count = thread.as_ref().unwrap().page_count as i32;
				pages.insert(-(page_count - page as i32), None);

				if pages
					.iter()
					.filter(|(id, _)| **id >= 0)
					.all(|(_, posts)| posts.is_some())
				{
					use std::mem::take;

					let pages = take(pages);
					let mut loc = take(loc);
					let thread = thread.take().unwrap();
					let flags = *flags;
					self.feed_sync_state = Synced {
						feed: loc.feed.clone(),
						pages: pages
							.keys()
							.map(|id| (*id as u32, true))
							.collect(),
					};

					let mut s = state::get_mut();
					self.trigger(&Change::ThreadList);
					self.trigger(&Change::Thread(thread_id));
					s.threads.insert(thread.id, thread);

					for p in pages
						.into_iter()
						.filter(|(id, _)| *id >= 0)
						.map(|(_, p)| p.unwrap().into_iter())
						.flatten()
					{
						self.trigger(&Change::Post(p.id));
						s.register_post(p);
					}

					// Normalize page number
					match &mut loc.feed {
						FeedID::Thread { page, .. } => {
							if *page < 0 {
								*page += page_count;
							}
						}
						_ => unreachable!(),
					};

					self.set_location_no_sync(&mut *s, loc, flags);
				}
			}
			Synced { feed, pages } if feed.as_u64() == thread_id => {
				pages.insert(page, true);

				self.trigger(&Change::Thread(thread_id));
				let mut s = state::get_mut();
				for p in posts {
					self.trigger(&Change::Post(p.id));
					s.register_post(p);
				}
			}
			_ => (),
		};
	}

	/// Register threads passed from the thread index feed
	fn register_threads(&mut self, threads: Vec<ThreadWithPosts>) {
		match &mut self.feed_sync_state {
			FeedSyncState::Receiving { loc, flags, .. }
				if loc.feed.as_u64() == 0 =>
			{
				let loc = std::mem::take(loc);
				let flags = *flags;
				self.feed_sync_state = FeedSyncState::Synced {
					feed: loc.feed.clone(),
					pages: Default::default(),
				};

				let s = &mut *state::get_mut();
				for t in threads {
					self.register_thread(s, t);
				}
				self.set_location_no_sync(s, loc, flags);
			}
			_ => (),
		};
	}

	/// Register thread in app state
	fn register_thread(&mut self, s: &mut super::State, t: ThreadWithPosts) {
		self.trigger(&Change::ThreadList);
		self.trigger(&Change::Thread(t.thread.id));

		s.threads.insert(t.thread.id, t.thread);
		for (_, p) in t.posts {
			self.trigger(&Change::Post(p.id));
			s.register_post(p);
		}
	}

	/// Fetch feed data from server, if needed.
	/// Returns, if a fetch is currently in progress.
	fn try_sync_feed(&mut self, new: &Location, flags: u8) -> bool {
		use crate::connection::{Connection, Request};
		use common::Encoder;

		let new_feed_num = new.feed.as_u64();

		// Clear any previous feed sync state, if feed changed
		match &mut self.feed_sync_state {
			// Already receiving data
			FeedSyncState::Receiving { loc, pages, .. }
				if loc.feed.as_u64() == new_feed_num =>
			{
				// Propagate non-feed updates
				*loc = new.clone();

				match &new.feed {
					FeedID::Thread { page, .. } => {
						if let Entry::Vacant(e) = pages.entry(*page) {
							// Requested another page
							e.insert(None);
							send(MessageType::Page, page);
						}
					}
					_ => (),
				};

				true
			}
			// If feed did not change, this is a page navigation within the
			// same feed. Keep the init data as there won't be any new received.
			FeedSyncState::Synced { feed, .. }
				if feed.as_u64() == new_feed_num =>
			{
				false
			}
			_ => util::with_logging(|| {
				use crate::connection::encode_msg;

				let mut e = Encoder::default();
				let mut pages = HashMap::new();

				encode_msg(&mut e, MessageType::Synchronize, &new_feed_num)?;

				match &new.feed {
					FeedID::Thread { page, .. } => {
						encode_msg(&mut e, MessageType::Page, page)?;
						pages.insert(*page, None);
					}
					_ => (),
				};

				Connection::dispatcher().send(Request::Send {
					is_open_post_manipulation: false,
					message: e.finish()?,
				});
				self.feed_sync_state = FeedSyncState::Receiving {
					loc: new.clone(),
					flags,
					pages,
					thread: None,
				};
				Ok(true)
			}),
		}
	}
}

/// Navigate to the app to a different location
pub fn navigate_to(loc: Location) {
	Agent::dispatcher().send(Request::NavigateTo {
		loc,
		flags: PUSH_STATE | SET_STATE | SCROLL_TO_FOCUSED,
	});
}
