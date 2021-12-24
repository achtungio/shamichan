use super::state;
use common::payloads::Thread;
use yew::{html, Component, ComponentLink, Html};

/// Central thread container
pub struct Threads {
	app_state: state::StateBridge,

	#[allow(unused)]
	link: ComponentLink<Self>,
}

impl Component for Threads {
	comp_message_rerender! {}
	comp_no_props! {}

	fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
		use state::Change;

		Self {
			app_state: state::hook(
				&link,
				vec![Change::Location, Change::ThreadList],
				|| (),
			),
			link: link,
		}
	}

	fn view(&self) -> Html {
		use super::thread as view;
		use state::FeedID;

		match &self.app_state.get().location.feed {
			FeedID::Catalog => {
				html! {
					<span>{"TODO"}</span>
				}
			}
			FeedID::Index => {
				let s = self.app_state.get();
				let mut threads: Vec<&Thread> = s.threads.values().collect();
				// TODO: Different sort orders
				threads
					.sort_unstable_by_key(|t| std::cmp::Reverse(t.bumped_on));

				let mut w = Vec::with_capacity(threads.len() * 2);
				for (i, t) in threads.into_iter().enumerate() {
					if i != 0 {
						w.push(html! {
							<hr />
						});
					}
					w.push(html! {
						<view::Thread
							id=t.id pages=view::PostSet::Last5Posts
						/>
					});
				}

				html! {
					<section>
						{w.into_iter().collect::<Html>()}
					</section>
				}
			}
			FeedID::Thread { id, page } => {
				if page < &0 {
					html! {}
				} else {
					let page = *page as u32;
					let mut pages = vec![page];
					if page > 0 {
						pages.insert(0, page - 1);
					}
					if page < self.app_state.get().page_count(id) - 1 {
						pages.push(page + 1);
					}

					html! {
						<view::Thread id=id pages=view::PostSet::Pages(pages) />
					}
				}
			}
		}
	}
}
