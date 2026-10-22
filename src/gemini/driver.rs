use std::time::Duration;
use tracing::{debug, info};

use super::selectors::*;
use crate::cdp::CdpClient;
use crate::error::{AxiError, Result};

/// High-level automation driver for the Gemini Web App.
pub struct GeminiDriver {
    client: CdpClient,
}

impl GeminiDriver {
    /// Creates a driver wrapping an already-connected CDP client.
    pub fn new(client: CdpClient) -> Self {
        Self { client }
    }

    /// Discovers an open Gemini tab (or opens one), connects via WebSocket CDP,
    /// enables required domains, and waits until the web app is ready.
    pub async fn connect_and_prepare(
        cdp_endpoint: &str,
        preferred_url: Option<&str>,
        ready_timeout: Duration,
    ) -> Result<Self> {
        let client = CdpClient::connect_to_target(cdp_endpoint, preferred_url).await?;
        debug!("Connected to Chrome target");

        client.enable_standard_domains().await?;

        let driver = Self::new(client);

        // Check if navigation is needed
        let target_url = preferred_url.unwrap_or(BASE_APP_URL);
        let current_href = driver
            .client
            .evaluate_string("window.location.href")
            .await?
            .unwrap_or_default();

        if !current_href.contains("gemini.google.com") {
            debug!(
                current = %current_href,
                target = %target_url,
                "Current tab not on Gemini; navigating"
            );
            driver.client.navigate(target_url).await?;
        }

        driver.wait_until_ready(ready_timeout).await?;
        Ok(driver)
    }

    /// Access the underlying CDP client.
    pub fn client(&self) -> &CdpClient {
        &self.client
    }

    /// Waits until the prompt input area is rendered and interactive.
    pub async fn wait_until_ready(&self, timeout: Duration) -> Result<()> {
        debug!("Waiting for Gemini web app to be interactive");
        self.client
            .wait_for_condition(
                "Gemini app readiness (prompt input visible)",
                &is_app_ready_js(),
                Duration::from_millis(250),
                timeout,
            )
            .await
    }

    /// Selects an AI model from the model picker (e.g. "argon", "pro", "flash").
    pub async fn select_model(&self, model: &str, timeout: Duration) -> Result<()> {
        info!(model = %model, "Selecting model in Gemini Web App");
        let start = std::time::Instant::now();
        let js = select_model_js(model);

        while start.elapsed() < timeout {
            if self.client.evaluate_bool(&js).await? {
                tokio::time::sleep(Duration::from_millis(400)).await;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }

        // Note: proceed even if model picker was not found (user might already be on desired model)
        debug!(
            model = %model,
            "Model picker option not explicitly confirmed; proceeding with current active model"
        );
        Ok(())
    }

    /// Inserts the prompt into the Gemini editor and submits it.
    pub async fn submit_prompt(&self, prompt: &str) -> Result<()> {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Err(AxiError::EmptyPrompt);
        }

        debug!("Setting prompt text into editor");
        let insert_js = insert_prompt_js(trimmed);
        let inserted = self.client.evaluate_bool(&insert_js).await?;
        if !inserted {
            return Err(AxiError::ElementNotFound(
                "prompt input editor element".into(),
            ));
        }

        tokio::time::sleep(Duration::from_millis(150)).await;

        // Check if send button is ready; if not, type a space via CDP Input.insertText
        // to guarantee native event activation on live Gemini Web App
        let send_check_js = r#"(() => {
            const selectors = [
                "button[aria-label*=\"Enviar\" i]",
                "button[aria-label*=\"Send\" i]",
                "gem-icon-button.send-button button",
                "button.send-button",
                ".send-button button"
            ];
            for (const sel of selectors) {
                const el = document.querySelector(sel);
                if (el && !el.disabled) return true;
            }
            return false;
        })()"#;

        if !self
            .client
            .evaluate_bool(send_check_js)
            .await
            .unwrap_or(false)
        {
            let _ = self.client.insert_text(" ").await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        debug!("Clicking send button");
        let send_js = click_send_js();
        let sent = self.client.evaluate_bool(&send_js).await?;
        if !sent {
            return Err(AxiError::ElementNotFound(
                "send button or send button disabled".into(),
            ));
        }

        Ok(())
    }

    /// Polls until response generation completes and extracts the final Markdown.
    pub async fn wait_for_response(&self, timeout: Duration) -> Result<String> {
        debug!("Waiting for response generation to complete");
        let start = std::time::Instant::now();

        // 1. Wait briefly for generation to begin (stop button visible or send button disabled)
        let streaming_js = is_streaming_js();
        let mut started = false;
        while start.elapsed() < Duration::from_secs(5) {
            if self.client.evaluate_bool(&streaming_js).await? {
                started = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        debug!(started = started, "Generation stream status detected");

        // 2. Poll until streaming indicator is gone and response is complete
        while start.elapsed() < timeout {
            let is_busy = self.client.evaluate_bool(&streaming_js).await?;
            if !is_busy {
                // Ensure DOM has settled
                tokio::time::sleep(Duration::from_millis(300)).await;
                let extract_js = extract_response_markdown_js();
                if let Some(md) = self.client.evaluate_string(&extract_js).await?
                    && !md.trim().is_empty()
                {
                    return Ok(md.trim().to_string());
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }

        Err(AxiError::Timeout(
            timeout.as_secs(),
            "Gemini response completion".into(),
        ))
    }

    /// Executes a standard chat prompt, optionally selecting a model, and returns the response Markdown.
    pub async fn run_chat(
        &self,
        prompt: &str,
        model: Option<&str>,
        timeout: Duration,
    ) -> Result<String> {
        if let Some(m) = model {
            self.select_model(m, Duration::from_secs(5)).await?;
        }

        self.submit_prompt(prompt).await?;
        self.wait_for_response(timeout).await
    }

    /// Executes a Deep Research workflow:
    /// 1. Submits research request.
    /// 2. Waits for the Research Plan to appear.
    /// 3. Approves the research plan.
    /// 4. Waits for multi-step research synthesis to finish.
    /// 5. Extracts the complete dossier in Markdown.
    pub async fn run_research(&self, prompt: &str, timeout: Duration) -> Result<String> {
        info!("Initiating Deep Research run");
        self.submit_prompt(prompt).await?;

        // Wait for plan generation (up to 45 seconds)
        let plan_timeout = Duration::from_secs(45);
        debug!("Waiting for Deep Research plan to appear");
        let start_plan = std::time::Instant::now();
        let plan_js = check_research_plan_js();
        let mut plan_ready = false;

        while start_plan.elapsed() < plan_timeout {
            if self.client.evaluate_bool(&plan_js).await? {
                plan_ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }

        if plan_ready {
            debug!("Deep Research plan detected; auto-approving plan");
            let approve_js = approve_research_plan_js();
            let _ = self.client.evaluate_bool(&approve_js).await;
            tokio::time::sleep(Duration::from_millis(500)).await;
        } else {
            debug!(
                "No explicit research plan confirmation detected; checking if research started directly"
            );
        }

        // Wait for research synthesis to complete
        debug!("Waiting for Deep Research synthesis to complete");
        let start_research = std::time::Instant::now();
        let is_running_js = is_research_in_progress_js();
        let is_stream_js = is_streaming_js();

        // Give research at least 3 seconds before assuming completion
        tokio::time::sleep(Duration::from_secs(3)).await;

        while start_research.elapsed() < timeout {
            let running = self.client.evaluate_bool(&is_running_js).await?
                || self.client.evaluate_bool(&is_stream_js).await?;

            if !running {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let extract_js = extract_response_markdown_js();
                if let Some(dossier) = self.client.evaluate_string(&extract_js).await?
                    && !dossier.trim().is_empty()
                {
                    return Ok(dossier.trim().to_string());
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        Err(AxiError::Timeout(
            timeout.as_secs(),
            "Deep Research synthesis and dossier generation".into(),
        ))
    }

    /// Interacts with a custom Gem by ID/slug.
    pub async fn run_gem(&self, gem_id: &str, prompt: &str, timeout: Duration) -> Result<String> {
        let gem_url = format!("{GEM_URL_PREFIX}{gem_id}");
        info!(gem_id = %gem_id, url = %gem_url, "Interacting with Custom Gem");

        let current_href = self
            .client
            .evaluate_string("window.location.href")
            .await?
            .unwrap_or_default();

        if !current_href.contains(gem_id) {
            self.client.navigate(&gem_url).await?;
            self.wait_until_ready(Duration::from_secs(30)).await?;
        }

        self.submit_prompt(prompt).await?;
        self.wait_for_response(timeout).await
    }
}
