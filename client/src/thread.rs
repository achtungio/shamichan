use super::{buttons, comp_util, post::posting, state};
use std::collections::HashSet;
use yew::{html, Html, Properties};

/// Central thread container
pub type Thread = comp_util::HookedComponent<Inner>;

#[derive(Default)]
pub struct Inner {}

/// Posts to display in a thread
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum PostSet {
	/// Display OP + last 5 posts
	Last5Posts,

	/// Display OP + selected pages
	Pages(Vec<u32>),
}

impl Default for PostSet {
	#[inline]
	fn default() -> Self {
		Self::Last5Posts
	}
}

#[derive(Clone, Properties, Eq, PartialEq, Debug)]
pub struct Props {
	pub id: u64,
	pub pages: PostSet,
}

impl comp_util::HookedComponentInner for Inner {
	type Message = ();
	type Properties = Props;

	fn update_message() -> Self::Message {
		()
	}

	#[inline]
	fn subscribe_to(props: &Self::Properties) -> Vec<state::Change> {
		vec![state::Change::Thread(props.id)]
	}

	#[inline]
	fn update(
		&mut self,
		_: &mut comp_util::Ctx<Self>,
		_: Self::Message,
	) -> bool {
		true
	}

	fn view(&self, c: &comp_util::Ctx<Self>) -> Html {
		use super::post::ThreadPost;
		use PostSet::*;

		// TODO: Filter hidden posts
		let posts: Vec<u64> = match &c.props().pages {
			Last5Posts => {
				let mut v = Vec::with_capacity(5);
				let page_count = c
					.app_state()
					.threads
					.get(&c.props().id)
					.map(|t| t.page_count)
					.unwrap_or(1);
				self.read_page_posts(c, &mut v, page_count - 1, true);
				if v.len() < 5 && page_count > 1 {
					self.read_page_posts(c, &mut v, page_count - 2, true);
				}

				v.sort_unstable();
				if v.len() > 5 {
					v = v[v.len() - 5..].iter().copied().collect();
				}

				// Always display OP
				v.insert(0, c.props().id);

				v
			}
			Pages(pages) => {
				let mut s = HashSet::<u64>::with_capacity(300);
				for p in pages.iter() {
					self.read_page_posts(c, &mut s, *p, false);
				}

				// Render pinned posts from any page
				s.extend(
					c.app_state()
						.pinned_posts
						.keys()
						.filter(|id| id != &&c.props().id),
				);

				let mut v: Vec<u64> = s.into_iter().collect();
				v.sort_unstable();
				v
			}
		};

		html! {
			<section class="thread-container" key=c.props().id>
				{
					for posts.into_iter().map(|id| {
						html! {
							<ThreadPost id=id />
						}
					})
				}
				<ReplyButton thread=c.props().id />
			</section>
		}
	}
}

impl Inner {
	/// Read the post IDs of a page, excluding the OP, into dst
	fn read_page_posts(
		&self,
		c: &comp_util::Ctx<Self>,
		dst: &mut impl Extend<u64>,
		page: u32,
		exclude_op: bool,
	) {
		if let Some(posts) = c
			.app_state()
			.posts_by_thread_page
			.get_by_key(&(c.props().id, page))
		{
			if exclude_op {
				dst.extend(
					posts.iter().filter(|id| **id != c.props().id).copied(),
				);
			} else {
				dst.extend(posts.iter().copied());
			}
		}
	}
}

#[derive(Properties, Eq, PartialEq, Clone)]
struct ReplyProps {
	thread: u64,
}

struct ReplyButton {
	props: ReplyProps,
	link: yew::ComponentLink<Self>,
	posting: Box<dyn yew::agent::Bridge<posting::Agent>>,
	state: posting::State,
}

enum ReplyMessage {
	SetState(posting::State),
	Clicked,
	NOP,
}

impl yew::Component for ReplyButton {
	super::comp_prop_change! {ReplyProps}
	type Message = ReplyMessage;

	fn create(props: Self::Properties, link: yew::ComponentLink<Self>) -> Self {
		use yew::agent::Bridged;

		Self {
			props,
			posting: posting::Agent::bridge(link.callback(|msg| match msg {
				posting::Response::State(s) => ReplyMessage::SetState(s),
				_ => ReplyMessage::NOP,
			})),
			link,
			state: Default::default(),
		}
	}

	fn update(&mut self, msg: Self::Message) -> bool {
		use ReplyMessage::*;

		match msg {
			SetState(s) => {
				self.state = s;
				true
			}
			Clicked => {
				self.posting
					.send(posting::Request::OpenDraft(self.props.thread));
				false
			}
			NOP => false,
		}
	}

	fn view(&self) -> yew::Html {
		match self.state {
			posting::State::Ready => html! {
				<buttons::AsideButton
					text="reply"
					on_click=self.link.callback(|e: yew::events::MouseEvent| {
						if e.button() == 0 {
							ReplyMessage::Clicked
						} else {
							ReplyMessage::NOP
						}
					})
				/>
			},
			_ => html! {},
		}
	}
}
