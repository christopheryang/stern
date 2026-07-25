use leptos::prelude::*;

#[component]
pub fn Sidebar(
    active_tab: ReadSignal<String>,
    set_active_tab: WriteSignal<String>,
) -> impl IntoView {
    let tabs = vec![
        ("chat".to_string(), "Chat".to_string()),
        ("secrets".to_string(), "Secrets".to_string()),
        ("documents".to_string(), "Documents".to_string()),
    ];

    view! {
        <nav style="width:220px;background:#16213e;border-right:1px solid #2a2a4a;display:flex;flex-direction:column;flex-shrink:0;">
            <div style="padding:20px 16px;border-bottom:1px solid #2a2a4a;">
                <h1 style="font-size:18px;font-weight:700;">"Stern"</h1>
            </div>
            <ul style="list-style:none;padding:8px;">
                {tabs.into_iter().map(move |(id, label)| {
                    let id_for_style = id.clone();
                    let id_for_click = id;
                    view! {
                        <li>
                            <button
                                style=move || {
                                    let active = active_tab.get() == id_for_style;
                                    let bg = if active { "#e94560" } else { "transparent" };
                                    let color = if active { "white" } else { "#a0a0b0" };
                                    format!(
                                        "display:block;width:100%;text-align:left;padding:10px 12px;border:none;background:{bg};color:{color};font-size:14px;cursor:pointer;border-radius:6px;transition:background 0.15s;",
                                    )
                                }
                                on:click=move |_| set_active_tab.set(id_for_click.clone())
                            >
                                {label}
                            </button>
                        </li>
                    }
                }).collect_view()}
            </ul>
        </nav>
    }
}
