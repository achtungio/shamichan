// For html! macro
#![recursion_limit = "1024"]

#[macro_use]
mod lang;
#[macro_use]
mod banner;
#[macro_use]
mod util;
#[macro_use]
mod comp_util;
mod buttons;
mod connection;
mod mouse;
mod page_selector;
mod post;
mod state;
mod thread;
mod thread_index;
mod time;
mod tool_panel;
mod user_bg;

use wasm_bindgen::prelude::*;
use yew::{html, Bridge, Bridged, Component, ComponentLink, Html};

// TODO: infinite scrolling with some floating widget to enable page jumping;
// put the OP in there too; transition it into the widget with an animation 

struct App {
	dragging: bool,

	// Keep here to load global state first and never drop the agents
	app_state: state::StateBridge,
	#[allow(unused)]
	mouse: Box<dyn Bridge<mouse::Agent>>,
}

enum Message {
	DraggingChange(bool),
	NOP,
}

impl Component for App {
	comp_no_props! {}
	type Message = Message;

	#[cold]
	fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
		let mut s = Self {
			app_state: state::hook(&link, vec![], || Message::NOP),
			mouse: mouse::Agent::bridge(link.callback(|msg| match msg {
				mouse::Response::IsDragging(d) => Message::DraggingChange(d),
				_ => Message::NOP,
			})),
			dragging: false,
		};
		s.app_state.send(state::Request::FetchFeed(
			s.app_state.get().location.clone(),
		));

		s
	}

	fn update(&mut self, msg: Self::Message) -> bool {
		match msg {
			Message::NOP => false,
			Message::DraggingChange(d) => {
				self.dragging = d;
				true
			}
		}
	}

	fn view(&self) -> Html {
		let mut cls = vec![];
		if self.dragging {
			cls.push("dragging");
		}

		html! {
			<section class=cls>
				<user_bg::Background />
				<div class="overlay-container">
					<banner::Banner />
					// z-index increases down
					<div class="overlay" id="post-form-overlay">
						<post::posting::PostForm
							id=self.app_state.get().open_post_id.unwrap_or(0)
						/>
					</div>
					<div class="overlay" id="modal-overlay">
						// TODO: modals
					</div>
					<div class="overlay" id="hover-overlay">
						// TODO: hover previews (post and image)
					</div>
				</div>
				<section id="main">
					<tool_panel::Panel is_top=true />
					<hr />
					<thread_index::Threads />
					<hr />
					<tool_panel::Panel />
				</section>
			</section>
		}
	}
}

#[wasm_bindgen(start)]
pub async fn main_js() -> util::Result {
	console_error_panic_hook::set_once();
	wasm_logger::init(wasm_logger::Config::new(if cfg!(debug_assertions) {
		log::Level::Debug
	} else {
		log::Level::Error
	}));

	let (err1, err2) =
		futures::future::join(state::init(), lang::load_language_pack()).await;
	err1?;
	err2?;

	yew::start_app::<App>();

	Ok(())
}
