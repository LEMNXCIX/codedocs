use crate::components::layout::Layout;
use leptos::logging::log;
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    view! {
       <Layout />
    }
}
