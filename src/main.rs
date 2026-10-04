mod actions;
mod app;
mod components;
mod shortcuts;
mod state;
mod types;
mod utils;
mod watch;

use app::*;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App /> })
}
