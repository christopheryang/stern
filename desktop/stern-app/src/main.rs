use leptos::prelude::*;
use stern_ui::app_shell::AppShell;

mod api;

fn main() {
    leptos::mount::mount_to_body(|| {
        view! { <AppShell /> }
    });
}
