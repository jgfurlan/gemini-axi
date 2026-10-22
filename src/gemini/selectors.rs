//! Gemini Web App DOM Selectors, UI Strings, and Evaluation Snippets.
//!
//! Calibrated against live Google Gemini web frontend (Angular / Material UI components).
//! Uses stable ARIA labels, roles, and data-test-id attributes ahead of generated CSS classes.

/// Base URL for the Gemini Web App.
pub const BASE_APP_URL: &str = "https://gemini.google.com/app";

/// URL prefix for Gemini Custom Gems.
pub const GEM_URL_PREFIX: &str = "https://gemini.google.com/gems/";

/// Secondary URL prefix for Gemini Custom Gems.
pub const GEM_URL_APP_PREFIX: &str = "https://gemini.google.com/app/gems/";

/// Selectors for locating the prompt input textbox.
pub const PROMPT_INPUT_SELECTORS: &[&str] = &[
    "rich-textarea .ql-editor[contenteditable=\"true\"]",
    "rich-textarea div[contenteditable=\"true\"][role=\"textbox\"]",
    "div[contenteditable=\"true\"][role=\"textbox\"][aria-label*=\"Gemini\" i]",
    "div[contenteditable=\"true\"][role=\"textbox\"]",
    "rich-textarea div[contenteditable=\"true\"]",
    "textarea",
];

/// Selectors for the Send / Submit prompt button.
pub const SEND_BUTTON_SELECTORS: &[&str] = &[
    "button[aria-label*=\"Enviar\" i]",
    "button[aria-label*=\"Send\" i]",
    "gem-icon-button.send-button button",
    "button.send-button",
    ".send-button button",
    "button[data-test-id=\"send-button\"]",
];

/// Selectors for the Stop Response button or active streaming spinner.
/// When any of these exist or are visible, generation is considered in progress.
pub const STOP_BUTTON_SELECTORS: &[&str] = &[
    "button[aria-label*=\"Parar resposta\" i]",
    "button[aria-label*=\"Stop response\" i]",
    "button[aria-label*=\"Parar\" i]",
    "button[aria-label*=\"Stop\" i]",
    "gem-icon-button.stop-button button",
    "button.stop-button",
    ".stop-button button",
    "mat-progress-spinner",
    "mat-progress-bar",
];

/// Selectors for opening the model picker dropdown (e.g. for Argon, Pro, Flash).
pub const MODEL_PICKER_TRIGGER_SELECTORS: &[&str] = &[
    "button[data-test-id=\"bard-mode-menu-button\"]",
    "[data-test-id=\"bard-mode-menu-button\"]",
    "button[aria-label*=\"seletor de modo\" i]",
    "button[aria-label*=\"mode\" i]",
    "button.input-area-switch",
    "[data-test-id=\"model-selector\"]",
    ".model-picker-button",
];

/// Selectors for options inside the model picker dropdown menu.
pub const MODEL_OPTION_SELECTORS: &[&str] = &[
    "[role=\"menuitem\"]",
    "gem-menu-item[data-mode-id]",
    "gem-menu-item[data-test-id^=\"bard-mode-option\"]",
    "gem-menu-item",
    "[role=\"menuitemradio\"]",
    "mat-option",
    ".model-option",
];

/// Selectors for triggering or toggling Deep Research mode.
pub const DEEP_RESEARCH_TRIGGER_SELECTORS: &[&str] = &[
    "button[aria-label*=\"Deep Research\" i]",
    "button[aria-label*=\"Pesquisa profunda\" i]",
    "button[aria-label*=\"Pesquisa aprofundada\" i]",
    "[data-test-id=\"deep-research-toggle\"]",
    ".deep-research-toggle",
];

/// Selectors for the Deep Research plan container.
pub const DEEP_RESEARCH_PLAN_SELECTORS: &[&str] = &[
    ".research-plan",
    "[data-test-id=\"research-plan\"]",
    ".deep-research-plan",
    "div.plan-container",
];

/// Selectors for the button to approve / start the research plan.
pub const DEEP_RESEARCH_APPROVE_BUTTON_SELECTORS: &[&str] = &[
    "button[aria-label*=\"Start research\" i]",
    "button[aria-label*=\"Iniciar pesquisa\" i]",
    "button[aria-label*=\"Aprovar\" i]",
    "button[data-test-id=\"start-research-button\"]",
    "button[data-test-id=\"approve-plan-button\"]",
];

/// Selectors for Deep Research active progress indicator.
pub const DEEP_RESEARCH_PROGRESS_SELECTORS: &[&str] = &[
    ".research-progress",
    ".research-step",
    "[data-test-id=\"research-status\"]",
];

/// Selectors for the model's response container.
pub const RESPONSE_CONTAINER_SELECTORS: &[&str] = &[
    "message-content .markdown",
    "message-content",
    ".model-response-text message-content",
    "model-response message-content",
    ".model-response-text",
    "model-response",
    ".response-container",
    "div[data-message-author-role=\"model\"]",
];

/// Selectors for the Copy Response Markdown / Text button.
pub const COPY_BUTTON_SELECTORS: &[&str] = &[
    "button[aria-label*=\"Copiar resposta\" i]",
    "button[aria-label*=\"Copiar comando\" i]",
    "button[aria-label*=\"Copiar\" i]",
    "button[aria-label*=\"Copy\" i]",
    "button[data-test-id=\"copy-button\"]",
];

/// Generates a JS expression that checks if the Gemini app is loaded and ready.
pub fn is_app_ready_js() -> String {
    let selectors_json = serde_json::to_string(PROMPT_INPUT_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const selectors = {selectors_json};
            for (const sel of selectors) {{
                const el = document.querySelector(sel);
                if (el && (el.offsetWidth > 0 || el.offsetHeight > 0 || el.getClientRects().length > 0)) {{
                    return true;
                }}
            }}
            return false;
        }})()"#
    )
}

/// Generates a JS expression that sets prompt text into the active prompt editor and triggers input events.
/// Avoids TrustedHTML violations on live Gemini by utilizing Quill editor API or execCommand.
pub fn insert_prompt_js(prompt: &str) -> String {
    let prompt_json = serde_json::to_string(prompt).unwrap_or_default();
    let selectors_json = serde_json::to_string(PROMPT_INPUT_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const text = {prompt_json};
            const selectors = {selectors_json};
            let target = null;
            for (const sel of selectors) {{
                const el = document.querySelector(sel);
                if (el) {{
                    target = el;
                    break;
                }}
            }}
            if (!target) return false;

            target.focus();
            if (target.tagName.toLowerCase() === 'textarea' || target.tagName.toLowerCase() === 'input') {{
                target.value = text;
                target.dispatchEvent(new Event('input', {{ bubbles: true }}));
                target.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }}

            // Contenteditable / Quill editor
            const rich = target.closest('rich-textarea');
            if (rich && rich.__quill) {{
                rich.__quill.setText(text);
                rich.dispatchEvent(new Event('input', {{ bubbles: true }}));
                rich.dispatchEvent(new Event('change', {{ bubbles: true }}));
            }} else {{
                document.execCommand('selectAll', false, null);
                document.execCommand('delete', false, null);
                document.execCommand('insertText', false, text);
            }}
            target.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: text }}));
            target.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return true;
        }})()"#
    )
}

/// Generates a JS expression to click the send button.
pub fn click_send_js() -> String {
    let selectors_json = serde_json::to_string(SEND_BUTTON_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const selectors = {selectors_json};
            for (const sel of selectors) {{
                const el = document.querySelector(sel);
                if (el && !el.disabled) {{
                    el.click();
                    return true;
                }}
            }}
            return false;
        }})()"#
    )
}

/// Generates a JS expression to check if Gemini is currently generating / streaming a response.
pub fn is_streaming_js() -> String {
    let stop_selectors_json = serde_json::to_string(STOP_BUTTON_SELECTORS).unwrap_or_default();
    let send_selectors_json = serde_json::to_string(SEND_BUTTON_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const stopSelectors = {stop_selectors_json};
            for (const sel of stopSelectors) {{
                const matches = document.querySelectorAll(sel);
                for (const el of matches) {{
                    if (el && (el.offsetWidth > 0 || el.offsetHeight > 0)) {{
                        if (!el.closest('bard-sidenav, side-navigation-v2')) {{
                            return true;
                        }}
                    }}
                }}
            }}
            const sendSelectors = {send_selectors_json};
            for (const sel of sendSelectors) {{
                const el = document.querySelector(sel);
                if (el && el.disabled) {{
                    return true;
                }}
            }}
            return false;
        }})()"#
    )
}

/// Generates a JS expression to open the model selector and pick a model by name.
pub fn select_model_js(model_name: &str) -> String {
    let model_name_json = serde_json::to_string(model_name).unwrap_or_default();
    let trigger_selectors_json =
        serde_json::to_string(MODEL_PICKER_TRIGGER_SELECTORS).unwrap_or_default();
    let option_selectors_json = serde_json::to_string(MODEL_OPTION_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const targetModel = {model_name_json}.toLowerCase();
            const triggers = {trigger_selectors_json};
            let triggerEl = null;
            for (const sel of triggers) {{
                const el = document.querySelector(sel);
                if (el) {{
                    triggerEl = el;
                    break;
                }}
            }}
            if (triggerEl) {{
                triggerEl.click();
            }}

            const options = {option_selectors_json};
            for (const sel of options) {{
                const elements = document.querySelectorAll(sel);
                for (const el of elements) {{
                    const text = (el.textContent || '').toLowerCase();
                    const aria = (el.getAttribute('aria-label') || '').toLowerCase();
                    if (text.includes(targetModel) || aria.includes(targetModel)) {{
                        el.click();
                        return true;
                    }}
                }}
            }}
            return false;
        }})()"#
    )
}

/// Generates a JS expression to check if a Deep Research plan is awaiting approval.
pub fn check_research_plan_js() -> String {
    let approve_selectors_json =
        serde_json::to_string(DEEP_RESEARCH_APPROVE_BUTTON_SELECTORS).unwrap_or_default();
    let plan_selectors_json =
        serde_json::to_string(DEEP_RESEARCH_PLAN_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const approveSelectors = {approve_selectors_json};
            for (const sel of approveSelectors) {{
                const el = document.querySelector(sel);
                if (el && (el.offsetWidth > 0 || el.offsetHeight > 0)) {{
                    return true;
                }}
            }}
            // Check button text containing 'Start research' or 'Iniciar pesquisa'
            const buttons = document.querySelectorAll('button');
            for (const b of buttons) {{
                const t = (b.textContent || '').trim().toLowerCase();
                if (t.includes('start research') || t.includes('iniciar pesquisa') || t.includes('aprovar plano')) {{
                    return true;
                }}
            }}
            const planSelectors = {plan_selectors_json};
            for (const sel of planSelectors) {{
                const el = document.querySelector(sel);
                if (el) return true;
            }}
            return false;
        }})()"#
    )
}

/// Generates a JS expression to approve the Deep Research plan.
pub fn approve_research_plan_js() -> String {
    let approve_selectors_json =
        serde_json::to_string(DEEP_RESEARCH_APPROVE_BUTTON_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const approveSelectors = {approve_selectors_json};
            for (const sel of approveSelectors) {{
                const el = document.querySelector(sel);
                if (el && !el.disabled) {{
                    el.click();
                    return true;
                }}
            }}
            const buttons = document.querySelectorAll('button');
            for (const b of buttons) {{
                const t = (b.textContent || '').trim().toLowerCase();
                if (t.includes('start research') || t.includes('iniciar pesquisa') || t.includes('aprovar plano')) {{
                    b.click();
                    return true;
                }}
            }}
            return false;
        }})()"#
    )
}

/// Generates a JS expression to check if Deep Research is currently running.
pub fn is_research_in_progress_js() -> String {
    let progress_selectors_json =
        serde_json::to_string(DEEP_RESEARCH_PROGRESS_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const progressSelectors = {progress_selectors_json};
            for (const sel of progressSelectors) {{
                const matches = document.querySelectorAll(sel);
                for (const el of matches) {{
                    if (el && (el.offsetWidth > 0 || el.offsetHeight > 0)) {{
                        if (!el.closest('bard-sidenav, side-navigation-v2')) {{
                            return true;
                        }}
                    }}
                }}
            }}
            // Check for progress indicators or spinners inside main chat area
            const spinners = document.querySelectorAll('mat-progress-bar, mat-progress-spinner, .sparkle-spinner');
            for (const s of spinners) {{
                if (s && (s.offsetWidth > 0 || s.offsetHeight > 0)) {{
                    if (!s.closest('bard-sidenav, side-navigation-v2')) {{
                        return true;
                    }}
                }}
            }}
            return false;
        }})()"#
    )
}

/// Generates a JS expression to extract the latest model response formatted as clean Markdown.
pub fn extract_response_markdown_js() -> String {
    let response_selectors_json =
        serde_json::to_string(RESPONSE_CONTAINER_SELECTORS).unwrap_or_default();
    format!(
        r#"(() => {{
            const selectors = {response_selectors_json};
            let responseContainer = null;
            for (const sel of selectors) {{
                const matches = document.querySelectorAll(sel);
                if (matches.length > 0) {{
                    responseContainer = matches[matches.length - 1];
                    break;
                }}
            }}
            if (!responseContainer) {{
                return null;
            }}

            function nodeToMarkdown(node) {{
                if (node.nodeType === Node.TEXT_NODE) {{
                    return node.textContent;
                }}
                if (node.nodeType !== Node.ELEMENT_NODE) {{
                    return '';
                }}
                const tag = node.tagName.toLowerCase();

                // Fenced code blocks
                if (tag === 'pre') {{
                    const code = node.querySelector('code') || node;
                    const lang = (code.className.match(/language-(\w+)/) || ['', ''])[1];
                    return '\n```' + lang + '\n' + (code.innerText || code.textContent).trim() + '\n```\n';
                }}
                if (tag === 'code') {{
                    return '`' + node.textContent + '`';
                }}

                // Headings
                if (/^h[1-6]$/.test(tag)) {{
                    const level = parseInt(tag[1], 10);
                    const prefix = '#'.repeat(level);
                    return '\n' + prefix + ' ' + node.textContent.trim() + '\n\n';
                }}

                // Lists
                if (tag === 'ul') {{
                    let text = '\n';
                    for (const li of node.children) {{
                        if (li.tagName.toLowerCase() === 'li') {{
                            text += '- ' + nodeToMarkdown(li).trim() + '\n';
                        }}
                    }}
                    return text + '\n';
                }}
                if (tag === 'ol') {{
                    let text = '\n';
                    let idx = 1;
                    for (const li of node.children) {{
                        if (li.tagName.toLowerCase() === 'li') {{
                            text += idx + '. ' + nodeToMarkdown(li).trim() + '\n';
                            idx++;
                        }}
                    }}
                    return text + '\n';
                }}

                // Paragraphs and Blockquotes
                if (tag === 'p') {{
                    let inner = '';
                    for (const child of node.childNodes) {{
                        inner += nodeToMarkdown(child);
                    }}
                    return inner.trim() ? inner.trim() + '\n\n' : '';
                }}
                if (tag === 'blockquote') {{
                    return '> ' + node.textContent.trim().replace(/\n/g, '\n> ') + '\n\n';
                }}

                // Formatting
                if (tag === 'strong' || tag === 'b') {{
                    return '**' + node.textContent + '**';
                }}
                if (tag === 'em' || tag === 'i') {{
                    return '*' + node.textContent + '*';
                }}
                if (tag === 'a') {{
                    return '[' + node.textContent + '](' + (node.getAttribute('href') || '#') + ')';
                }}
                if (tag === 'hr') {{
                    return '\n---\n\n';
                }}

                let result = '';
                for (const child of node.childNodes) {{
                    result += nodeToMarkdown(child);
                }}
                return result;
            }}

            const markdown = nodeToMarkdown(responseContainer).trim();
            if (markdown.length > 0) {{
                return markdown;
            }}
            return responseContainer.innerText || responseContainer.textContent || '';
        }})()"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_arrays_are_populated() {
        assert!(!PROMPT_INPUT_SELECTORS.is_empty());
        assert!(!SEND_BUTTON_SELECTORS.is_empty());
        assert!(!STOP_BUTTON_SELECTORS.is_empty());
        assert!(!MODEL_PICKER_TRIGGER_SELECTORS.is_empty());
        assert!(!MODEL_OPTION_SELECTORS.is_empty());
        assert!(!DEEP_RESEARCH_TRIGGER_SELECTORS.is_empty());
        assert!(!DEEP_RESEARCH_PLAN_SELECTORS.is_empty());
        assert!(!DEEP_RESEARCH_APPROVE_BUTTON_SELECTORS.is_empty());
        assert!(!RESPONSE_CONTAINER_SELECTORS.is_empty());
    }

    #[test]
    fn test_js_snippet_generation() {
        let ready_js = is_app_ready_js();
        assert!(ready_js.contains("document.querySelector"));

        let insert_js = insert_prompt_js("Hello \"world\"\nNew line");
        assert!(insert_js.contains("Hello \\\"world\\\"\\nNew line"));

        let send_js = click_send_js();
        assert!(send_js.contains("el.click()"));

        let model_js = select_model_js("argon");
        assert!(model_js.contains("\"argon\""));

        let plan_js = check_research_plan_js();
        assert!(plan_js.contains("start research"));

        let extract_js = extract_response_markdown_js();
        assert!(extract_js.contains("nodeToMarkdown"));
    }
}
