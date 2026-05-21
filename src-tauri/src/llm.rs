use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::scanner::ScanCandidate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub batch_size: u32,
    pub timeout_secs: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            base_url: "http://localhost:1234".to_string(),
            model: "local-model".to_string(),
            batch_size: 5,
            timeout_secs: 180,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmDebugEntry {
    pub batch_index: u32,
    pub batch_size: u32,
    pub target_url: String,
    pub request_body: String,
    pub response_format_used: String,
    pub http_status: Option<u16>,
    pub response_body: String,
    pub parsed_content: String,
    pub reasoning_content: String,
    pub max_tokens_used: u32,
    pub error: Option<String>,
    pub fallback_attempted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmRefineResult {
    pub success: bool,
    pub error: Option<String>,
    pub candidates: Vec<RefinedCandidate>,
    pub debug: Vec<LlmDebugEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinedCandidate {
    pub port: u16,
    pub file: String,
    pub line: u32,
    pub context: String,
    pub keyword: String,
    pub is_port: bool,
    pub software: Option<String>,
    pub role: Option<String>,
    pub confidence: f32,
    pub reason: Option<String>,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    response_format: ResponseFormat,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
    json_schema: JsonSchemaSpec,
}

#[derive(Serialize)]
struct JsonSchemaSpec {
    name: &'static str,
    strict: bool,
    schema: serde_json::Value,
}

fn build_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "is_port": { "type": "boolean" },
                        "software": { "type": ["string", "null"] },
                        "role": { "type": ["string", "null"] },
                        "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                        "reason": { "type": ["string", "null"] }
                    },
                    "required": ["is_port", "software", "role", "confidence", "reason"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["items"],
        "additionalProperties": false
    })
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct ItemsWrapper {
    items: Vec<ItemVerdict>,
}

#[derive(Deserialize, Default, Clone)]
struct ItemVerdict {
    #[serde(default)]
    is_port: bool,
    #[serde(default)]
    software: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    confidence: f32,
    #[serde(default)]
    reason: Option<String>,
}

const SYSTEM_PROMPT: &str = r#"You analyze candidate port-number references extracted from local configuration files for a port-management UI.

For each candidate, decide whether the number is really a TCP/UDP network port assignment (NOT e.g. a max-connections value, a version number, a sample/example, a timestamp, a buffer size, a percentage, or a count). If yes, infer which software/service owns it and what role the port plays, using the file name and line content as hints.

Rules:
- Be strict. If unsure, set is_port=false with reason.
- Valid ports: 1-65535. Numbers outside this range are NOT ports.
- "software" should be a short product name (e.g. "MySQL", "Redis", "nginx", "Spring Boot app"). If you can't tell, set null.
- "role" is one short phrase: "listen", "admin UI", "client connect", "metrics", "replication", etc. Null if uncertain.
- "confidence" is 0.0 to 1.0.
- Return ONLY a JSON object: {"items": [...]} with items in the SAME ORDER and SAME LENGTH as the input array."#;

fn build_user_prompt(batch: &[ScanCandidate]) -> String {
    let mut s = String::from("Candidates to evaluate (one JSON object per line, do NOT echo the input):\n");
    for (i, c) in batch.iter().enumerate() {
        let file_basename = std::path::Path::new(&c.file)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| c.file.clone());
        let line_obj = serde_json::json!({
            "i": i,
            "port": c.port,
            "file": file_basename,
            "line_text": c.context,
            "keyword_match": c.keyword
        });
        s.push_str(&line_obj.to_string());
        s.push('\n');
    }
    s.push_str("\nReturn {\"items\":[{\"is_port\":bool,\"software\":string|null,\"role\":string|null,\"confidence\":number,\"reason\":string|null}, ...]} with exactly ");
    s.push_str(&batch.len().to_string());
    s.push_str(" entries in order.");
    s
}

fn extract_json_block(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    for (i, b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let taken: String = s.chars().take(max).collect();
        format!("{}... [+{} chars truncated]", taken, s.chars().count() - max)
    }
}

#[derive(Serialize)]
struct ChatRequestText<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    response_format: TextResponseFormat,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct TextResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

fn attempt_request(
    client: &reqwest::blocking::Client,
    url: &str,
    body: serde_json::Value,
) -> (Option<u16>, String, String, Option<String>) {
    let body_str = serde_json::to_string(&body).unwrap_or_default();
    match client.post(url).json(&body).send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            match resp.text() {
                Ok(text) => (Some(status), body_str, text, None),
                Err(e) => (Some(status), body_str, String::new(), Some(format!("read body failed: {}", e))),
            }
        }
        Err(e) => (None, body_str, String::new(), Some(format!("HTTP send failed: {}", e))),
    }
}

fn parse_content_to_verdicts(content: &str) -> Result<Vec<ItemVerdict>, String> {
    let json_block = extract_json_block(content)
        .ok_or_else(|| format!("no JSON object found in model output (len={}): {}",
            content.len(), truncate_chars(content, 300)))?;
    let wrapper: ItemsWrapper = serde_json::from_str(json_block)
        .map_err(|e| format!("JSON didn't match items schema: {} (block: {})",
            e, truncate_chars(json_block, 300)))?;
    Ok(wrapper.items)
}

fn extract_choice_content(body: &str) -> Result<(String, Option<String>), String> {
    let parsed: ChatResponse = serde_json::from_str(body)
        .map_err(|e| format!("invalid chat response shape: {} (body: {})", e, truncate_chars(body, 400)))?;
    let choice = parsed
        .choices
        .first()
        .ok_or_else(|| "model returned no choices".to_string())?;
    Ok((choice.message.content.clone(), choice.message.reasoning_content.clone()))
}

fn refine_batch(
    client: &reqwest::blocking::Client,
    cfg: &LlmConfig,
    batch: &[ScanCandidate],
    batch_index: u32,
) -> (Result<Vec<ItemVerdict>, String>, LlmDebugEntry) {
    let url = format!("{}/v1/chat/completions", cfg.base_url.trim_end_matches('/'));
    // Allocate generously: 2000 base + 1500 per candidate (reasoning models burn budget fast).
    let max_tokens: u32 = (2000u32.saturating_add((batch.len() as u32).saturating_mul(1500))).min(32768);
    let mut dbg = LlmDebugEntry {
        batch_index,
        batch_size: batch.len() as u32,
        target_url: url.clone(),
        request_body: String::new(),
        response_format_used: "json_schema".to_string(),
        http_status: None,
        response_body: String::new(),
        parsed_content: String::new(),
        reasoning_content: String::new(),
        max_tokens_used: max_tokens,
        error: None,
        fallback_attempted: false,
    };

    // Append `/no_think` — Qwen 3 family responds to this literal directive even when
    // chat_template_kwargs.enable_thinking is ignored by fine-tunes.
    let user_prompt = format!("{}\n\n/no_think", build_user_prompt(batch));
    let disable_thinking = Some(serde_json::json!({ "enable_thinking": false }));

    // Attempt 1: json_schema strict mode (with Qwen-style thinking disabled)
    let schema_body = serde_json::to_value(ChatRequest {
        model: &cfg.model,
        messages: vec![
            ChatMessage { role: "system", content: SYSTEM_PROMPT.to_string() },
            ChatMessage { role: "user", content: user_prompt.clone() },
        ],
        temperature: 0.0,
        response_format: ResponseFormat {
            kind: "json_schema",
            json_schema: JsonSchemaSpec {
                name: "PortVerdicts",
                strict: true,
                schema: build_response_schema(),
            },
        },
        max_tokens,
        chat_template_kwargs: disable_thinking.clone(),
    })
    .unwrap_or_default();

    let (status, req_body, resp_body, send_err) = attempt_request(client, &url, schema_body);
    dbg.http_status = status;
    dbg.request_body = truncate_chars(&req_body, 2000);
    dbg.response_body = truncate_chars(&resp_body, 4000);

    if let Some(e) = send_err {
        dbg.error = Some(e.clone());
        return (Err(e), dbg);
    }
    let status_code = status.unwrap_or(0);
    let mut try_text_fallback = false;
    if !(200..300).contains(&status_code) {
        // HTTP error — likely response_format not supported; try text fallback
        try_text_fallback = true;
        dbg.error = Some(format!("HTTP {} on json_schema attempt: {}", status_code, truncate_chars(&resp_body, 400)));
    } else {
        match extract_choice_content(&resp_body) {
            Ok((content, reasoning)) => {
                dbg.parsed_content = truncate_chars(&content, 4000);
                if let Some(r) = reasoning.as_ref() {
                    dbg.reasoning_content = truncate_chars(r, 4000);
                }
                if !content.trim().is_empty() {
                    match parse_content_to_verdicts(&content) {
                        Ok(items) => return (Ok(items), dbg),
                        Err(e) => {
                            dbg.error = Some(format!("json_schema parse failed: {}", e));
                            try_text_fallback = true;
                        }
                    }
                } else if reasoning.as_ref().map(|s| !s.is_empty()).unwrap_or(false) {
                    dbg.error = Some(format!(
                        "json_schema 下 content 为空，但 reasoning_content 有 {} 字——模型在死命思考。已尝试 enable_thinking=false + /no_think 但被你这个微调模型忽略了。\n\n按优先级试：\n① LM Studio 左侧栏里这个模型的「Reasoning Effort」改成 Low / Off / Minimal；\n② 「LLM 设置」里把批大小改到 1 或 2 并保存（当前 max_tokens={}，可能仍不够它想完一批）；\n③ 换非推理模型（Qwen2.5-7B-Instruct / Llama-3-8B-Instruct / Mistral-7B-Instruct 都行）。",
                        reasoning.as_ref().map(|s| s.len()).unwrap_or(0),
                        max_tokens
                    ));
                    try_text_fallback = true;
                } else {
                    dbg.error = Some("json_schema returned empty content; falling back to text mode".to_string());
                    try_text_fallback = true;
                }
            }
            Err(e) => {
                dbg.error = Some(format!("json_schema shape error: {}", e));
                try_text_fallback = true;
            }
        }
    }

    if !try_text_fallback {
        return (Err(dbg.error.clone().unwrap_or_else(|| "unknown".to_string())), dbg);
    }

    // Attempt 2: text mode with explicit JSON instruction
    dbg.fallback_attempted = true;
    dbg.response_format_used = "text (fallback)".to_string();
    let text_body = serde_json::to_value(ChatRequestText {
        model: &cfg.model,
        messages: vec![
            ChatMessage { role: "system", content: SYSTEM_PROMPT.to_string() },
            ChatMessage {
                role: "user",
                content: format!(
                    "{}\n\nOUTPUT FORMAT: Reply with ONLY a JSON object, no prose, no markdown fences, NO chain-of-thought reasoning. Direct JSON output only. Schema: {{\"items\":[{{\"is_port\":bool,\"software\":string|null,\"role\":string|null,\"confidence\":number,\"reason\":string|null}}, ...]}}.",
                    user_prompt
                ),
            },
        ],
        temperature: 0.0,
        response_format: TextResponseFormat { kind: "text" },
        max_tokens,
        chat_template_kwargs: disable_thinking.clone(),
    })
    .unwrap_or_default();
    let (status2, req_body2, resp_body2, send_err2) = attempt_request(client, &url, text_body);
    dbg.http_status = status2;
    dbg.request_body = truncate_chars(&req_body2, 2000);
    dbg.response_body = truncate_chars(&resp_body2, 4000);
    if let Some(e) = send_err2 {
        let combined = format!("{}; text fallback: {}", dbg.error.clone().unwrap_or_default(), e);
        dbg.error = Some(combined.clone());
        return (Err(combined), dbg);
    }
    let status_code2 = status2.unwrap_or(0);
    if !(200..300).contains(&status_code2) {
        let combined = format!(
            "{}; text fallback HTTP {}: {}",
            dbg.error.clone().unwrap_or_default(),
            status_code2,
            truncate_chars(&resp_body2, 400)
        );
        dbg.error = Some(combined.clone());
        return (Err(combined), dbg);
    }
    match extract_choice_content(&resp_body2) {
        Ok((content, reasoning)) => {
            dbg.parsed_content = truncate_chars(&content, 4000);
            if let Some(r) = reasoning.as_ref() {
                dbg.reasoning_content = truncate_chars(r, 4000);
            }
            match parse_content_to_verdicts(&content) {
                Ok(items) => {
                    dbg.error = None;
                    (Ok(items), dbg)
                }
                Err(e) => {
                    let hint = if reasoning.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
                        && content.trim().is_empty()
                    {
                        format!(
                            " [推理模型 thinking 又把 {} 字写进 reasoning_content 了，content 仍是空。强烈建议把批大小降到 3，或换非推理模型]",
                            reasoning.as_ref().map(|s| s.len()).unwrap_or(0)
                        )
                    } else {
                        String::new()
                    };
                    let combined = format!(
                        "{}; text fallback parse: {}{}",
                        dbg.error.clone().unwrap_or_default(),
                        e,
                        hint
                    );
                    dbg.error = Some(combined.clone());
                    (Err(combined), dbg)
                }
            }
        }
        Err(e) => {
            let combined = format!(
                "{}; text fallback shape: {}",
                dbg.error.clone().unwrap_or_default(),
                e
            );
            dbg.error = Some(combined.clone());
            (Err(combined), dbg)
        }
    }
}

pub fn refine_all(cfg: &LlmConfig, candidates: Vec<ScanCandidate>) -> LlmRefineResult {
    if candidates.is_empty() {
        return LlmRefineResult {
            success: true,
            error: None,
            candidates: Vec::new(),
            debug: Vec::new(),
        };
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(cfg.timeout_secs as u64))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return LlmRefineResult {
                success: false,
                error: Some(format!("HTTP client build failed: {}", e)),
                candidates: Vec::new(),
                debug: Vec::new(),
            };
        }
    };

    let bsz = cfg.batch_size.max(1) as usize;
    let mut out = Vec::with_capacity(candidates.len());
    let mut debug = Vec::new();
    let mut first_err: Option<String> = None;

    for (b_idx, batch) in candidates.chunks(bsz).enumerate() {
        let (result, dbg) = refine_batch(&client, cfg, batch, b_idx as u32);
        debug.push(dbg);
        match result {
            Ok(verdicts) => {
                for (idx, cand) in batch.iter().enumerate() {
                    let v = verdicts.get(idx).cloned().unwrap_or_default();
                    out.push(RefinedCandidate {
                        port: cand.port,
                        file: cand.file.clone(),
                        line: cand.line,
                        context: cand.context.clone(),
                        keyword: cand.keyword.clone(),
                        is_port: v.is_port,
                        software: v.software.filter(|s| !s.trim().is_empty()),
                        role: v.role.filter(|s| !s.trim().is_empty()),
                        confidence: v.confidence.clamp(0.0, 1.0),
                        reason: v.reason.filter(|s| !s.trim().is_empty()),
                    });
                }
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("batch #{}: {}", b_idx, e));
                }
                // Push placeholder verdicts so candidate count stays aligned
                for cand in batch {
                    out.push(RefinedCandidate {
                        port: cand.port,
                        file: cand.file.clone(),
                        line: cand.line,
                        context: cand.context.clone(),
                        keyword: cand.keyword.clone(),
                        is_port: true, // don't auto-reject on LLM failure
                        software: None,
                        role: None,
                        confidence: 0.0,
                        reason: Some("LLM batch failed; treated as unverified".to_string()),
                    });
                }
            }
        }
    }

    LlmRefineResult {
        success: first_err.is_none(),
        error: first_err,
        candidates: out,
        debug,
    }
}

pub fn check_health(cfg: &LlmConfig) -> Result<String, String> {
    let url = format!("{}/v1/models", cfg.base_url.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("LM Studio responded {}", resp.status()));
    }
    let text = resp.text().unwrap_or_default();
    Ok(text)
}
