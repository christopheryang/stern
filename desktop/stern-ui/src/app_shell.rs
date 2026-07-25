use crate::components::chat::ChatView;
use crate::components::sidebar::Sidebar;
use leptos::prelude::*;

#[component]
pub fn AppShell() -> impl IntoView {
    let (active_tab, set_active_tab) = signal("chat".to_string());

    view! {
        <div style="display:flex;height:100vh;">
            <Sidebar active_tab=active_tab set_active_tab=set_active_tab />
            <main style="flex:1;display:flex;flex-direction:column;overflow:hidden;">
                {move || -> AnyView {
                    match active_tab.get().as_str() {
                        "chat" => view! { <ChatView /> }.into_any(),
                        "secrets" => view! {
                            <div style="padding:20px;">
                                <h2 style="font-size:18px;font-weight:600;">"Secrets"</h2>
                                <p style="color:#a0a0b0;margin-top:8px;">
                                    "Use the chat to store and retrieve secrets."
                                </p>
                            </div>
                        }.into_any(),
                        "documents" => view! {
                            <div style="padding:20px;">
                                <h2 style="font-size:18px;font-weight:600;">"Documents"</h2>
                                <p style="color:#a0a0b0;margin-top:8px;">
                                    "Use the chat to store and retrieve documents."
                                </p>
                            </div>
                        }.into_any(),
                        _ => view! { <ChatView /> }.into_any(),
                    }
                }}
            </main>
        </div>
    }
}
