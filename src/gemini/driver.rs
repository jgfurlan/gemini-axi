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
        client.ensure_active_and_focused().await?;

        let driver = Self::new(client);

        // Check if navigation is needed: if preferred_url is None, always ensure
        // a clean, fresh conversation at BASE_APP_URL with no prior messages.
        let target_url = preferred_url.unwrap_or(BASE_APP_URL);
        let current_href = driver
            .client
            .evaluate_string("window.location.href")
            .await?
            .unwrap_or_default();

        let query_count = driver
            .client
            .evaluate_u64("document.querySelectorAll('user-query').length")
            .await
            .unwrap_or(0);

        if !current_href.contains("gemini.google.com")
            || (preferred_url.is_none() && (current_href != BASE_APP_URL || query_count > 0))
        {
            debug!(
                current = %current_href,
                target = %target_url,
                query_count = query_count,
                "Current tab not on fresh Gemini conversation; navigating"
            );
            driver.client.navigate(target_url).await?;
        }

        driver.wait_until_ready(ready_timeout).await?;
        driver.client.ensure_active_and_focused().await?;
        Ok(driver)
    }

    /// Access the underlying CDP client.
    pub fn client(&self) -> &CdpClient {
        &self.client
    }

    /// Closes any dynamically created browser tab.
    pub async fn close(&self) -> Result<()> {
        self.client.close_target().await
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

    /// Enables Deep Research mode via the Tools menu.
    pub async fn enable_deep_research(&self, timeout: Duration) -> Result<()> {
        info!("Enabling Deep Research mode");
        let start = std::time::Instant::now();

        // 1. Check if already enabled
        let verify_js = is_deep_research_enabled_js();
        if self.client.evaluate_bool(&verify_js).await? {
            debug!("Deep Research already enabled");
            return Ok(());
        }

        // 2. Poll and advance through Tools -> Mais ferramentas -> Deep Research
        let step_js = r#"(() => {
            // (a) If Deep Research item is visible, click it
            let drBtn = Array.from(document.querySelectorAll('button, [role="menuitemcheckbox"], mat-list-item')).find(b =>
                (b.innerText && b.innerText.includes('Deep Research')) ||
                ((b.getAttribute('aria-label') || '').includes('Deep Research'))
            );
            if (drBtn) {
                drBtn.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
                drBtn.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
                drBtn.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
                drBtn.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
                drBtn.click();
                return true;
            }

            // (b) If Mais ferramentas is visible, click it to open submenu
            const moreBtn = document.querySelector('button[data-test-id="more-tools-button"], button[aria-label*="Mais ferramentas" i]') ||
                Array.from(document.querySelectorAll('button')).find(b => (b.innerText || '').includes('Mais ferramentas'));
            if (moreBtn) {
                moreBtn.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
                moreBtn.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
                moreBtn.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
                moreBtn.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
                moreBtn.click();
                return false;
            }

            // (c) If Tools menu is not yet open, click Tools button
            const toolsBtn = document.querySelector('button[aria-label*="ferramentas" i]') ||
                document.querySelector('button[aria-label*="tools" i]');
            if (toolsBtn) {
                toolsBtn.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
                toolsBtn.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
                toolsBtn.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
                toolsBtn.dispatchEvent(new MouseEvent('mouseup', { bubbles: true }));
                toolsBtn.click();
            }
            return false;
        })()"#;

        while start.elapsed() < timeout {
            let _ = self.client.evaluate_bool(step_js).await;
            tokio::time::sleep(Duration::from_millis(500)).await;

            if self.client.evaluate_bool(&verify_js).await.unwrap_or(false) {
                info!("Deep Research successfully enabled");
                return Ok(());
            }
        }

        if self.client.evaluate_bool(&verify_js).await? {
            return Ok(());
        }

        Err(AxiError::ElementNotFound(
            "Deep Research toggle item".into(),
        ))
    }

    /// Inserts the prompt into the Gemini editor, submits it, and verifies it appears in DOM.
    pub async fn submit_prompt(&self, prompt: &str) -> Result<()> {
        let trimmed = prompt.trim();
        if trimmed.is_empty() {
            return Err(AxiError::EmptyPrompt);
        }

        self.client.ensure_active_and_focused().await?;

        debug!("Focusing prompt input editor");
        let focus_js = r#"(() => {
            const el = document.querySelector('.ql-editor') ||
                       document.querySelector('rich-textarea [contenteditable="true"]') ||
                       document.querySelector('div[contenteditable="true"][role="textbox"]');
            if (el) {
                el.focus();
                const rich = el.closest('rich-textarea');
                if (rich && rich.__quill) {
                    rich.__quill.setText("");
                }
                return true;
            }
            return false;
        })()"#;

        if !self.client.evaluate_bool(focus_js).await? {
            let insert_js = insert_prompt_js(trimmed);
            if !self.client.evaluate_bool(&insert_js).await? {
                return Err(AxiError::ElementNotFound(
                    "prompt input editor element".into(),
                ));
            }
        } else {
            debug!("Inserting prompt text via CDP Input.insertText");
            self.client.insert_text(trimmed).await?;
        }

        tokio::time::sleep(Duration::from_millis(250)).await;

        // Check if Send button is ready (wait up to 3s)
        let send_check_js = r#"(() => {
            const selectors = [
                "button[aria-label*=\"Enviar mensagem\" i]",
                "button[aria-label*=\"Send message\" i]",
                "button[aria-label*=\"Enviar\" i]",
                "button[aria-label*=\"Send\" i]",
                "gem-icon-button.send-button.has-input button",
                ".send-button.has-input button",
                "button[data-test-id=\"send-button\"]",
                ".send-button button"
            ];
            for (const sel of selectors) {
                const el = document.querySelector(sel);
                if (el && !el.disabled) {
                    const aria = (el.getAttribute('aria-label') || '').toLowerCase();
                    if (!aria.includes('ditar') && !aria.includes('dictate')) {
                        return true;
                    }
                }
            }
            return false;
        })()"#;

        let start_check = std::time::Instant::now();
        let mut send_ready = false;
        while start_check.elapsed() < Duration::from_secs(3) {
            if self
                .client
                .evaluate_bool(send_check_js)
                .await
                .unwrap_or(false)
            {
                send_ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        if !send_ready {
            debug!("Send button not yet ready; typing a space fallback to trigger event");
            let _ = self.client.insert_text(" ").await;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }

        debug!("Clicking send button");
        let send_js = click_send_js();
        let sent = self.client.evaluate_bool(&send_js).await?;
        if !sent {
            return Err(AxiError::ElementNotFound(
                "send button or send button disabled".into(),
            ));
        }

        // Verify that the prompt was actually submitted into the DOM
        debug!("Verifying user query appeared in DOM");
        let verify_js = is_prompt_in_dom_js(trimmed);
        let start_verify = std::time::Instant::now();
        let mut submitted = false;
        while start_verify.elapsed() < Duration::from_secs(7) {
            if self.client.evaluate_bool(&verify_js).await? {
                submitted = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        if !submitted {
            return Err(AxiError::PromptNotSubmitted(trimmed.to_string()));
        }

        info!("Prompt successfully submitted and confirmed in conversation DOM");
        Ok(())
    }

    /// Polls until response generation completes and extracts the final Markdown.
    /// Rejects any response from before the prompt was submitted.
    pub async fn wait_for_response(
        &self,
        initial_response_count: usize,
        timeout: Duration,
    ) -> Result<String> {
        debug!(
            initial_count = initial_response_count,
            "Waiting for response generation to complete"
        );
        let start = std::time::Instant::now();

        // 1. Wait for generation to START (streaming active or new response element created)
        let streaming_js = is_streaming_js();
        let count_js = count_model_responses_js();
        let start_timeout = Duration::from_secs(10);
        let mut started = false;

        while start.elapsed() < start_timeout {
            let is_stream = self.client.evaluate_bool(&streaming_js).await?;
            let current_count = self.client.evaluate_u64(&count_js).await? as usize;
            if is_stream || current_count > initial_response_count {
                started = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        if !started {
            return Err(AxiError::GenerationDidNotStart(start_timeout.as_secs()));
        }

        debug!(started = started, "Generation stream start confirmed");

        // 2. Poll until streaming indicator is gone AND new response is extracted
        while start.elapsed() < timeout {
            let is_busy = self.client.evaluate_bool(&streaming_js).await?;
            if !is_busy {
                // Ensure DOM has settled
                tokio::time::sleep(Duration::from_millis(300)).await;
                let extract_js = extract_new_response_markdown_js(initial_response_count);
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

        let initial_count = self
            .client
            .evaluate_u64(&count_model_responses_js())
            .await? as usize;
        debug!(
            initial_count = initial_count,
            "Recorded initial response count before prompt"
        );

        self.submit_prompt(prompt).await?;
        self.wait_for_response(initial_count, timeout).await
    }

    /// Executes a Deep Research workflow:
    /// 1. Enables Deep Research mode (if starting fresh).
    /// 2. Submits research request.
    /// 3. Waits for the Research Plan to appear.
    /// 4. Approves the research plan by clicking "Iniciar pesquisa" / "Start research".
    /// 5. Waits for multi-step research synthesis to finish, reporting progress.
    /// 6. Extracts the complete dossier in Markdown, rejecting incomplete plans.
    pub async fn run_research(&self, prompt: &str, timeout: Duration) -> Result<String> {
        info!("Initiating Deep Research run");

        let plan_js = check_research_plan_js();
        let is_running_js = is_research_in_progress_js();

        // Check if completed dossier is already present (e.g. resuming completed conversation)
        let initial_extract_js = extract_research_report_js(0);
        if let Ok(Some(existing_dossier)) = self.client.evaluate_string(&initial_extract_js).await {
            let trimmed = existing_dossier.trim();
            let is_running = self
                .client
                .evaluate_bool(&is_running_js)
                .await
                .unwrap_or(false);
            if !trimmed.is_empty() && !is_running {
                info!("Completed Deep Research dossier already present on page");
                return Ok(trimmed.to_string());
            }
        }

        let plan_already_present = self.client.evaluate_bool(&plan_js).await.unwrap_or(false);
        let already_running = self
            .client
            .evaluate_bool(&is_running_js)
            .await
            .unwrap_or(false);

        let initial_count = if !plan_already_present && !already_running {
            if prompt.trim().is_empty() {
                return Err(AxiError::EmptyPrompt);
            }

            self.enable_deep_research(Duration::from_secs(10)).await?;

            let count = self
                .client
                .evaluate_u64(&count_model_responses_js())
                .await? as usize;
            debug!(
                initial_count = count,
                "Recorded initial response count before research prompt"
            );

            self.submit_prompt(prompt).await?;

            // Wait for plan generation (up to 90 seconds)
            let plan_timeout = Duration::from_secs(90);
            debug!("Waiting for Deep Research plan to appear");
            let start_plan = std::time::Instant::now();
            let mut plan_ready = false;

            while start_plan.elapsed() < plan_timeout {
                if self.client.evaluate_bool(&plan_js).await? {
                    plan_ready = true;
                    break;
                }
                if self.client.evaluate_bool(&is_running_js).await? {
                    // Started directly
                    break;
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }

            if !plan_ready && !self.client.evaluate_bool(&is_running_js).await? {
                return Err(AxiError::GenerationDidNotStart(plan_timeout.as_secs()));
            }

            count
        } else {
            debug!("Existing plan or running research detected on current page");
            0
        };

        // If plan is awaiting approval, click "Iniciar pesquisa" / "Start research"
        if self.client.evaluate_bool(&plan_js).await.unwrap_or(false) {
            info!("Deep Research plan detected; auto-approving research plan ('Iniciar pesquisa')");
            let approve_js = approve_research_plan_js();
            let mut approved = false;
            let start_approve = std::time::Instant::now();
            while start_approve.elapsed() < Duration::from_secs(10) {
                if self
                    .client
                    .evaluate_bool(&approve_js)
                    .await
                    .unwrap_or(false)
                {
                    approved = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            if !approved {
                debug!("Approve click dispatched; checking if research execution started");
            }
        }

        // Wait for research to START (up to min(30s, timeout))
        debug!("Waiting for Deep Research execution to start");
        let start_research_wait = std::time::Instant::now();
        let wait_start_timeout = std::cmp::min(Duration::from_secs(30), timeout);
        let mut started = false;
        while start_research_wait.elapsed() < wait_start_timeout {
            let running = self
                .client
                .evaluate_bool(&is_running_js)
                .await
                .unwrap_or(false);
            if running {
                started = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        if !started {
            // Check if still stuck on plan
            if self.client.evaluate_bool(&plan_js).await.unwrap_or(false) {
                return Err(AxiError::ResearchPlanNotCompleted);
            }
            return Err(AxiError::GenerationDidNotStart(
                wait_start_timeout.as_secs(),
            ));
        }

        info!("Deep Research execution confirmed; waiting for multi-step synthesis to complete");
        let start_synthesis = std::time::Instant::now();
        let progress_js = get_research_progress_js();
        let mut last_progress = String::new();

        while start_synthesis.elapsed() < timeout {
            // Monitor progress text in stderr for -v
            if let Ok(Some(prog)) = self.client.evaluate_string(&progress_js).await {
                let trimmed = prog.trim();
                if !trimmed.is_empty() && trimmed != last_progress {
                    info!("Deep Research: {}", trimmed);
                    last_progress = trimmed.to_string();
                }
            }

            let running = self
                .client
                .evaluate_bool(&is_running_js)
                .await
                .unwrap_or(false);
            if !running {
                // Let DOM settle
                tokio::time::sleep(Duration::from_secs(2)).await;
                let still_running = self
                    .client
                    .evaluate_bool(&is_running_js)
                    .await
                    .unwrap_or(false);
                if !still_running {
                    let extract_js = extract_research_report_js(initial_count);
                    if let Some(dossier) = self.client.evaluate_string(&extract_js).await? {
                        let trimmed = dossier.trim();
                        if !trimmed.is_empty() {
                            return Ok(trimmed.to_string());
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        // Timed out: check if only plan is present
        if self.client.evaluate_bool(&plan_js).await.unwrap_or(false) {
            return Err(AxiError::ResearchPlanNotCompleted);
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

        let initial_count = self
            .client
            .evaluate_u64(&count_model_responses_js())
            .await? as usize;

        self.submit_prompt(prompt).await?;
        self.wait_for_response(initial_count, timeout).await
    }
}
