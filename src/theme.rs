use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct ThemeContext {
    pub theme: ReadSignal<String>,
    pub set_theme: WriteSignal<String>,
}

pub fn normalize_theme(theme: &str) -> &'static str {
    match theme {
        "dark" => "dark",
        _ => "bambu",
    }
}

/// Apply the theme by setting `data-theme` on `<html>`.
pub fn apply_theme(theme: &str) {
    if let Some(window) = web_sys::window() {
        if let Some(doc) = window.document() {
            if let Some(html) = doc.document_element() {
                let _ = html.set_attribute("data-theme", normalize_theme(theme));
            }
        }
    }
}
