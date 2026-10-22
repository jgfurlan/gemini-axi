//! Read-only observations in a fresh, owned Gemini tab; no prompt is submitted.
use crate::{CdpClient, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub login: String,
    pub model: Option<String>,
    pub models: Vec<String>,
    pub gems: Vec<Gem>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Gem {
    pub id: String,
    pub name: String,
    pub url: String,
}

pub const STATE_JS: &str = r#"(() => {
    // gemini_axi_state: observational dashboard, also used after responses.
    const editor = document.querySelector('rich-textarea .ql-editor, rich-textarea [contenteditable="true"], [contenteditable="true"][role="textbox"]');
    const picker = document.querySelector('[data-test-id="bard-mode-menu-button"], [data-test-id="model-picker-button"], .model-picker-button, button[aria-label*="model" i], button[aria-label*="mode" i], button[aria-label*="seletor de modo" i]');
    const model = picker?.innerText?.trim() || picker?.getAttribute('aria-label')?.split(/No momento:|Currently:/).pop().trim() || null;
    const models = [...new Set(Array.from(document.querySelectorAll('gem-menu-item[data-mode-id], gem-menu-item[data-test-id^="bard-mode-option"], [data-test-id*="model-option"]')).map(e => (e.querySelector('.label')?.innerText || e.innerText.split('\n')[0]).trim()).filter(Boolean))];
    if (!models.length && model) models.push(model);
    const gems = Array.from(document.querySelectorAll('a[href*="/gem/"]')).map(a => ({id: a.getAttribute('href').split('/gem/')[1].split(/[?#/]/)[0], name: a.innerText.trim(), url: a.href})).filter((g,i,a) => g.id && a.findIndex(x => x.id === g.id) === i);
    return {login: editor || document.querySelector('a[href*="accounts.google.com/SignOutOptions"]') ? 'signed_in' : document.querySelector('a[href*="accounts.google.com/ServiceLogin"], a[href*="accounts.google.com/signin"]') ? 'signed_out' : 'unknown', model, models, gems};
})()"#;

pub async fn state(client: &CdpClient) -> Result<State> {
    let value = client.evaluate(STATE_JS).await?.unwrap_or_default();
    Ok(serde_json::from_value(value)?)
}
pub async fn inspect(endpoint: &str, gems: bool) -> Result<State> {
    let client = CdpClient::connect_to_target(endpoint, None).await?;
    let result = async {
        client.enable_standard_domains().await?;
        client.ensure_active_and_focused().await?;
        if gems { client.navigate("https://gemini.google.com/gems/view").await?; }
        let ready = if gems {
            "!!document.querySelector('main') && /Gems/i.test(document.querySelector('main').innerText) && !document.querySelector('mat-progress-spinner')"
        } else {
            "!!document.querySelector('button[data-test-id=\"bard-mode-menu-button\"], .model-picker-button, a[href*=\"accounts.google.com/ServiceLogin\"], a[href*=\"accounts.google.com/signin\"]')"
        };
        client.wait_for_condition("Gemini dashboard", ready, Duration::from_millis(200), Duration::from_secs(8)).await?;
        // Read available choices in our own tab. No model is selected.
        if !gems {
            let _ = client.evaluate("document.querySelector('[data-test-id=\"bard-mode-menu-button\"], [data-test-id=\"model-picker-button\"], .model-picker-button, button[aria-label*=\"seletor de modo\" i], button[aria-label*=\"mode\" i]')?.click()").await;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        state(&client).await
    }.await;
    let _ = client.close_target().await;
    result
}
