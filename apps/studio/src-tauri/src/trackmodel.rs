//! Which model writes a track, and how to reach it.
//!
//! By default the Studio asks our control plane, which holds an Anthropic key. Someone can
//! bring their own instead: any OpenAI-compatible endpoint (Groq, OpenRouter, OpenAI, a local
//! Ollama or LM Studio) or Anthropic itself. The Studio then calls it directly, with the same
//! prompt and schema the control plane uses, because both read `packages/track-protocol`.
//!
//! The key stays on this machine. It lives in its own file rather than in `config.json`, which
//! older managers rewrite without it and which the webview is sent whole, and it never goes
//! into an error, a log line or the usage report.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::trackllm::{Ask, Attempt, REJECTED};

pub const PROGRAM_SYSTEM: &str =
    include_str!("../../../../packages/track-protocol/program.system.md");
pub const PROGRAM_SCHEMA: &str =
    include_str!("../../../../packages/track-protocol/program.schema.json");
pub const SETTINGS_SYSTEM: &str =
    include_str!("../../../../packages/track-protocol/settings.system.md");
pub const SETTINGS_SCHEMA: &str =
    include_str!("../../../../packages/track-protocol/settings.schema.json");
pub const EDIT_SYSTEM: &str = include_str!("../../../../packages/track-protocol/edit.system.md");
pub const PAINT_EDIT_SYSTEM: &str =
    include_str!("../../../../packages/track-protocol/paint-edit.system.md");
pub const PAINT_EDIT_SCHEMA: &str =
    include_str!("../../../../packages/track-protocol/paint-edit.schema.json");

/// What the app asks a model for. See `control-plane/src/trackgen.ts` for the same protocols.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// The whole lap, drawn by the model and measured here. Only a strong model manages it.
    Program,
    /// The character only; `tracklayout` draws the lap. Small enough for any model.
    Settings,
    /// The complete open lap, changed in place from a rider's instruction.
    TrackEdit,
    /// A small plan of layer operations, resolved against the loaded model in the webview.
    PaintEdit,
}

impl Protocol {
    /// The control plane's name for it, and the key its answer comes back under.
    pub fn wire(self) -> &'static str {
        match self {
            Protocol::Program => "program",
            Protocol::Settings => "settings",
            Protocol::TrackEdit => "trackEdit",
            Protocol::PaintEdit => "paintEdit",
        }
    }

    pub fn system(self) -> &'static str {
        match self {
            Protocol::Program => PROGRAM_SYSTEM,
            Protocol::Settings => SETTINGS_SYSTEM,
            Protocol::TrackEdit => EDIT_SYSTEM,
            Protocol::PaintEdit => PAINT_EDIT_SYSTEM,
        }
    }

    fn schema_text(self) -> &'static str {
        match self {
            Protocol::Program => PROGRAM_SCHEMA,
            Protocol::Settings => SETTINGS_SCHEMA,
            Protocol::TrackEdit => PROGRAM_SCHEMA,
            Protocol::PaintEdit => PAINT_EDIT_SCHEMA,
        }
    }

    pub fn schema(self) -> Value {
        serde_json::from_str(self.schema_text()).expect("packages/track-protocol holds valid JSON")
    }

    fn schema_name(self) -> &'static str {
        match self {
            Protocol::Program => "track_program",
            Protocol::Settings => "track_settings",
            Protocol::TrackEdit => "track_edit",
            Protocol::PaintEdit => "paint_edit",
        }
    }

    /// Output ceiling. The program's is the control plane's, for the same reason: a full lap
    /// plus the thinking that lays it out. Settings get more than nineteen fields need because
    /// a reasoning model (gpt-oss, say) spends its thinking out of the same allowance, and a
    /// free tier counts the ceiling against its per-minute limit, so no more than that.
    fn max_tokens(self) -> u32 {
        match self {
            Protocol::Program => 32000,
            Protocol::Settings => 3000,
            Protocol::TrackEdit => 32000,
            Protocol::PaintEdit => 3000,
        }
    }
}

/// The conversation, in the control plane's words, so a model reached directly is asked
/// exactly what one reached through us is.
fn turns(protocol: Protocol, brief: &str, attempt: &Attempt) -> Vec<Value> {
    let mut out = vec![json!({ "role": "user", "content": brief })];
    // Settings are always legal once clamped, so they are never sent back to be fixed.
    if matches!(protocol, Protocol::Program | Protocol::TrackEdit) {
        if let Some(previous) = attempt.previous.as_deref() {
            if !attempt.problems.is_empty() {
                let list: Vec<String> =
                    attempt.problems.iter().take(40).map(|p| format!("- {p}")).collect();
                out.push(json!({ "role": "assistant", "content": previous }));
                out.push(json!({
                    "role": "user",
                    "content": format!(
                        "The app built that and measured it. These are wrong:\n\n{}\n\nSend the whole program again with those fixed.",
                        list.join("\n")
                    ),
                }));
            }
        }
    } else if protocol == Protocol::PaintEdit && !attempt.problems.is_empty() {
        if let Some(previous) = attempt.previous.as_deref() {
            out.push(json!({ "role": "assistant", "content": previous }));
        }
        out.push(json!({
            "role": "user",
            "content": format!(
                "That action plan was rejected: {}. Send a corrected plan for the original request.",
                attempt.problems.join("; ")
            ),
        }));
    }
    out
}

/// Which API shape the endpoint speaks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// `/chat/completions`: Groq, OpenRouter, OpenAI, Ollama, LM Studio and most others.
    #[default]
    OpenAi,
    /// `/v1/messages`.
    Anthropic,
}

/// A model of the user's own, as saved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackModel {
    pub kind: Kind,
    pub base_url: String,
    pub model: String,
    /// Empty for a local server that takes none.
    pub key: String,
}

/// What the webview is shown: everything but the key.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrackModelView {
    pub kind: Kind,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

impl TrackModel {
    pub fn view(&self) -> TrackModelView {
        TrackModelView {
            kind: self.kind,
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            has_key: !self.key.trim().is_empty(),
        }
    }

    pub fn usable(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.model.trim().is_empty()
    }

    /// Who is being asked, for messages. The host rather than the URL, which some people paste
    /// with a key in the query string.
    fn host(&self) -> String {
        reqwest::Url::parse(self.base_url.trim())
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "the model service".into())
    }

    /// Where to post. Takes the base URL each provider documents, and forgives one pasted
    /// with the endpoint already on it.
    fn url(&self) -> String {
        let base = self.base_url.trim().trim_end_matches('/');
        match self.kind {
            Kind::OpenAi if base.ends_with("/chat/completions") => base.into(),
            Kind::OpenAi => format!("{base}/chat/completions"),
            Kind::Anthropic if base.ends_with("/v1/messages") => base.into(),
            Kind::Anthropic if base.ends_with("/v1") => format!("{base}/messages"),
            Kind::Anthropic => format!("{base}/v1/messages"),
        }
    }

    /// Takes the key out of anything a provider says back. Some echo it in an auth error.
    fn scrub(&self, said: &str) -> String {
        let key = self.key.trim();
        if key.len() >= 8 {
            said.replace(key, "[key]")
        } else {
            said.to_string()
        }
    }
}

/// The file the model is saved in, next to `config.json`.
pub const FILE: &str = "studio-model.json";

/// The saved model, if there is one worth using.
pub fn load(dir: &Path) -> Option<TrackModel> {
    let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
    let model: TrackModel = serde_json::from_str(&text).ok()?;
    model.usable().then_some(model)
}

pub fn save(dir: &Path, model: &TrackModel) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FILE);
    std::fs::write(&path, serde_json::to_vec_pretty(model)?)
        .with_context(|| format!("writing {}", path.display()))?;
    // Readable by this user only: it holds a key someone pays for.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn clear(dir: &Path) -> Result<()> {
    match std::fs::remove_file(dir.join(FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// A model reached with the user's own key.
pub struct Direct {
    pub model: TrackModel,
}

impl Ask for Direct {
    async fn ask(&self, protocol: Protocol, brief: &str, attempt: &Attempt) -> Result<String> {
        let messages = turns(protocol, brief, attempt);
        match self.model.kind {
            Kind::OpenAi => self.openai(protocol, &messages).await,
            Kind::Anthropic => self.anthropic(protocol, &messages).await,
        }
    }
}

impl Direct {
    async fn post(&self, body: &Value) -> Result<(u16, String)> {
        let key = self.model.key.trim();
        let mut req = reqwest::Client::new()
            .post(self.model.url())
            .json(body)
            // A whole lap takes minutes, the same as through the control plane.
            .timeout(std::time::Duration::from_secs(600));
        req = match self.model.kind {
            Kind::OpenAi if key.is_empty() => req,
            Kind::OpenAi => req.bearer_auth(key),
            Kind::Anthropic => req
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01"),
        };
        let res = req
            .send()
            .await
            .map_err(|e| anyhow!("couldn't reach {}: {}", self.model.host(), e.without_url()))?;
        let status = res.status().as_u16();
        let text = res.text().await.context("reading the model's answer")?;
        Ok((status, text))
    }

    async fn openai(&self, protocol: Protocol, messages: &[Value]) -> Result<String> {
        let (mut status, mut text) =
            self.post(&openai_body(&self.model.model, protocol, messages, true)).await?;
        // Ollama, LM Studio and older gateways refuse `json_schema` outright. Plain JSON with
        // the schema in the prompt still works on them, and the app checks every answer anyway.
        if status == 400 && !text.contains("json_validate_failed") && mentions_format(&text) {
            log::info!("[trackmodel] {} takes no JSON schema; asking for plain JSON", self.model.host());
            (status, text) =
                self.post(&openai_body(&self.model.model, protocol, messages, false)).await?;
        }
        if !(200..300).contains(&status) {
            return refusal(&self.model, protocol, status, &text);
        }
        let v: Value = serde_json::from_str(&text)
            .with_context(|| format!("{} sent something unreadable", self.model.host()))?;
        let choice = &v["choices"][0];
        if choice["finish_reason"] == "length" {
            bail!(cut_off(protocol, &self.model.host()));
        }
        answer(choice["message"]["content"].as_str())
    }

    async fn anthropic(&self, protocol: Protocol, messages: &[Value]) -> Result<String> {
        let (status, text) =
            self.post(&anthropic_body(&self.model.model, protocol, messages)).await?;
        if !(200..300).contains(&status) {
            return refusal(&self.model, protocol, status, &text);
        }
        let v: Value = serde_json::from_str(&text)
            .with_context(|| format!("{} sent something unreadable", self.model.host()))?;
        match v["stop_reason"].as_str() {
            Some("refusal") => return Ok(format!("{REJECTED}the model declined that brief")),
            Some("max_tokens") => bail!(cut_off(protocol, &self.model.host())),
            _ => {}
        }
        let text = v["content"]
            .as_array()
            .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
            .and_then(|b| b["text"].as_str());
        answer(text)
    }
}

/// One small request, to say whether the address, the model and the key work before a track
/// is asked for.
pub async fn check(model: &TrackModel) -> Result<()> {
    let messages = [json!({ "role": "user", "content": "Reply with the word ok." })];
    let body = match model.kind {
        Kind::OpenAi => {
            json!({ "model": model.model, "messages": messages, "max_completion_tokens": 32 })
        }
        Kind::Anthropic => json!({ "model": model.model, "max_tokens": 32, "messages": messages }),
    };
    let (status, text) = Direct { model: model.clone() }.post(&body).await?;
    if (200..300).contains(&status) {
        return Ok(());
    }
    // A complaint about the answer still means the key and the model worked.
    refusal(model, Protocol::Settings, status, &text).map(|_| ())
}

fn openai_body(model: &str, protocol: Protocol, messages: &[Value], strict: bool) -> Value {
    let mut system = protocol.system().to_string();
    let format = if strict {
        json!({
            "type": "json_schema",
            "json_schema": { "name": protocol.schema_name(), "strict": true, "schema": protocol.schema() },
        })
    } else {
        system.push_str(
            "\n\nAnswer with one JSON object that matches this JSON Schema, and nothing else:\n",
        );
        system.push_str(protocol.schema_text());
        json!({ "type": "json_object" })
    };
    let mut all = vec![json!({ "role": "system", "content": system })];
    all.extend_from_slice(messages);
    json!({
        "model": model,
        "messages": all,
        "response_format": format,
        // The current name; `max_tokens` is refused by OpenAI's reasoning models.
        "max_completion_tokens": protocol.max_tokens(),
    })
}

fn anthropic_body(model: &str, protocol: Protocol, messages: &[Value]) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": protocol.max_tokens(),
        "system": protocol.system(),
        "messages": messages,
        "output_config": {
            "format": { "type": "json_schema", "schema": anthropic_schema(protocol.schema()) },
        },
    });
    // The control plane's rule, see `ask` in trackgen.ts: Haiku 4.5 takes a fixed budget and
    // refuses `effort`; the rest think adaptively. Settings have no arithmetic to think about.
    if matches!(protocol, Protocol::Program | Protocol::TrackEdit) {
        if model.starts_with("claude-haiku") {
            body["thinking"] = json!({ "type": "enabled", "budget_tokens": 4000 });
        } else {
            body["thinking"] = json!({ "type": "adaptive" });
            body["output_config"]["effort"] = json!("low");
        }
    }
    body
}

/// The schema as the Anthropic SDK sends it, which is what the control plane sends: an `enum`
/// becomes words in the description. Each value would be another alternative in the compiled
/// grammar, and the program schema sits right at that grammar's ceiling.
fn anthropic_schema(mut node: Value) -> Value {
    match &mut node {
        Value::Object(o) => {
            if let Some(values) = o.remove("enum") {
                let words = format!("{{enum: {values}}}");
                let description = match o.get("description").and_then(Value::as_str) {
                    Some(d) => format!("{d}\n\n{words}"),
                    None => words,
                };
                o.insert("description".into(), Value::String(description));
            }
            for v in o.values_mut() {
                *v = anthropic_schema(v.take());
            }
        }
        Value::Array(a) => {
            for v in a.iter_mut() {
                *v = anthropic_schema(v.take());
            }
        }
        _ => {}
    }
    node
}

/// The JSON in an answer. A model asked for plain JSON sometimes fences it anyway.
fn answer(text: Option<&str>) -> Result<String> {
    let text = text.unwrap_or_default().trim();
    if text.is_empty() {
        return Ok(format!("{REJECTED}the model sent an empty answer"));
    }
    Ok(match (text.find('{'), text.rfind('}')) {
        (Some(a), Some(b)) if a < b => text[a..=b].to_string(),
        _ => text.to_string(),
    })
}

fn mentions_format(body: &str) -> bool {
    let body = body.to_lowercase();
    ["response_format", "json_schema", "structured"].iter().any(|w| body.contains(w))
}

fn cut_off(protocol: Protocol, host: &str) -> String {
    match protocol {
        Protocol::Program | Protocol::TrackEdit => format!(
            "the answer from {host} ran past {} tokens and was cut off. A whole lap needs a model \
             with a long output; Settings only needs a few hundred tokens.",
            protocol.max_tokens()
        ),
        Protocol::Settings | Protocol::PaintEdit => {
            format!("the answer from {host} ran past {} tokens and was cut off", protocol.max_tokens())
        }
    }
}

/// What a provider's error means for the loop. `Ok` is the model's mistake, an answer to send
/// back and have fixed, the same as the control plane's 422. `Err` stops, because asking
/// again would fail the same way.
fn refusal(model: &TrackModel, protocol: Protocol, status: u16, body: &str) -> Result<String> {
    let host = model.host();
    let said = model.scrub(&provider_message(body));
    match status {
        // Groq checks the answer against the schema itself and reports a miss as a 400.
        400 if body.contains("json_validate_failed") => {
            Ok(format!("{REJECTED}that didn't fit the schema: {said}"))
        }
        422 => Ok(format!("{REJECTED}{said}")),
        401 | 403 => bail!("{host} refused the key: {said}"),
        404 => bail!(
            "{host} doesn't know the model \"{}\", or the address is wrong: {said}",
            model.model
        ),
        413 if matches!(protocol, Protocol::Program | Protocol::TrackEdit) => bail!(
            "a whole lap is too large for this model's limit on {host}. Free tiers allow a few \
             thousand tokens a minute; Settings only fits in that. ({said})"
        ),
        413 => bail!("that's too large for this model's limit on {host}: {said}"),
        429 if matches!(protocol, Protocol::Program | Protocol::TrackEdit) => bail!(
            "{host} is rate limiting this key: {said}. A whole lap is tens of thousands of \
             tokens; Settings only is far smaller."
        ),
        429 => bail!("{host} is rate limiting this key: {said}"),
        _ => bail!("{host} answered {status}: {said}"),
    }
}

/// The reason in an error body. OpenAI, Groq, OpenRouter and Anthropic all say
/// `{"error": {"message": …}}`; Ollama says `{"error": "…"}`.
fn provider_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["error"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groq() -> TrackModel {
        TrackModel {
            kind: Kind::OpenAi,
            base_url: "https://api.groq.com/openai/v1/".into(),
            model: "openai/gpt-oss-120b".into(),
            key: "gsk_secret_key_123456".into(),
        }
    }

    #[test]
    fn the_endpoint_is_found_from_the_base_url_people_paste() {
        assert_eq!(groq().url(), "https://api.groq.com/openai/v1/chat/completions");
        let mut m = groq();
        m.base_url = "http://localhost:11434/v1/chat/completions".into();
        assert_eq!(m.url(), "http://localhost:11434/v1/chat/completions");
        m.kind = Kind::Anthropic;
        for base in ["https://api.anthropic.com", "https://api.anthropic.com/v1", "https://api.anthropic.com/v1/messages/"] {
            m.base_url = base.into();
            assert_eq!(m.url(), "https://api.anthropic.com/v1/messages", "{base}");
        }
    }

    #[test]
    fn an_openai_request_asks_for_the_schema_strictly() {
        let body = openai_body("m", Protocol::Settings, &turns(Protocol::Settings, "sandy", &Attempt::default()), true);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "sandy");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["response_format"]["json_schema"]["schema"], Protocol::Settings.schema());
        assert_eq!(body["max_completion_tokens"], 3000);

        let loose = openai_body("m", Protocol::Settings, &[], false);
        assert_eq!(loose["response_format"]["type"], "json_object");
        assert!(loose["messages"][0]["content"].as_str().unwrap().contains("\"cornersPerKm\""));
    }

    #[test]
    fn retries_carry_the_answer_and_the_problems_for_document_edits() {
        let attempt = Attempt { previous: Some("{\"name\":\"x\"}".into()), problems: vec!["too wide".into()] };
        let program = turns(Protocol::Program, "b", &attempt);
        assert_eq!(program.len(), 3);
        assert_eq!(program[1]["role"], "assistant");
        assert!(program[2]["content"].as_str().unwrap().contains("- too wide"));
        assert_eq!(turns(Protocol::Settings, "b", &attempt).len(), 1);
        assert_eq!(turns(Protocol::TrackEdit, "b", &attempt).len(), 3);
        let paint = turns(Protocol::PaintEdit, "b", &attempt);
        assert_eq!(paint.len(), 3);
        assert!(paint[2]["content"].as_str().unwrap().contains("too wide"));
    }

    #[test]
    fn an_anthropic_request_matches_what_the_control_plane_sends() {
        let haiku = anthropic_body("claude-haiku-4-5", Protocol::Program, &[]);
        assert_eq!(haiku["thinking"]["type"], "enabled");
        assert!(haiku["output_config"].get("effort").is_none());
        let opus = anthropic_body("claude-opus-5", Protocol::Program, &[]);
        assert_eq!(opus["thinking"]["type"], "adaptive");
        assert_eq!(opus["output_config"]["effort"], "low");
        assert!(anthropic_body("claude-opus-5", Protocol::Settings, &[]).get("thinking").is_none());
        assert!(anthropic_body("claude-opus-5", Protocol::PaintEdit, &[]).get("thinking").is_none());

        // No enum reaches the grammar; the values survive as words.
        let schema = &haiku["output_config"]["format"]["schema"];
        assert!(!schema.to_string().contains("\"enum\""));
        assert!(schema["properties"]["features"]["items"]["properties"]["kind"]["description"]
            .as_str()
            .unwrap()
            .contains("stepUp"));

        // And the same for the discipline, which is the one thing on the settings schema that
        // has to reach the model as a closed list. It does not reach it as an `enum` — the
        // SDK's transform pops that off — so what the model is actually held to is this
        // sentence, and the prompt says the same thing again in words.
        let settings = anthropic_body("claude-haiku-4-5", Protocol::Settings, &[]);
        let settings = &settings["output_config"]["format"]["schema"];
        assert!(!settings.to_string().contains("\"enum\""));
        let says = settings["properties"]["discipline"]["description"].as_str().unwrap();
        for value in ["mx", "sx", "smx"] {
            assert!(says.contains(&format!("\"{value}\"")), "{value} is not in {says:?}");
        }
        assert!(settings["required"].as_array().unwrap().iter().any(|r| r == "discipline"));
    }

    #[test]
    fn errors_say_what_happened_and_never_carry_the_key() {
        let m = groq();
        let echoed = r#"{"error":{"message":"Invalid API Key gsk_secret_key_123456"}}"#;
        let e = refusal(&m, Protocol::Settings, 401, echoed).unwrap_err().to_string();
        assert!(e.contains("refused the key"), "{e}");
        assert!(!e.contains("gsk_secret_key_123456"), "{e}");

        let e = refusal(&m, Protocol::Program, 413, r#"{"error":{"message":"Request too large"}}"#)
            .unwrap_err()
            .to_string();
        assert!(e.contains("Settings only"), "{e}");

        // The model's mistakes go back to be fixed rather than stopping the loop.
        let again = refusal(&m, Protocol::Program, 400, r#"{"error":{"code":"json_validate_failed","message":"bad"}}"#).unwrap();
        assert!(again.starts_with(REJECTED));
        assert!(refusal(&m, Protocol::Program, 422, "{}").unwrap().starts_with(REJECTED));
        assert!(refusal(&m, Protocol::Program, 500, "boom").is_err());
    }

    #[test]
    fn a_fenced_answer_is_unwrapped() {
        assert_eq!(answer(Some("```json\n{\"a\":1}\n```")).unwrap(), "{\"a\":1}");
        assert!(answer(Some("  ")).unwrap().starts_with(REJECTED));
    }

    #[test]
    fn the_saved_model_round_trips_and_the_view_hides_the_key() {
        let dir = std::env::temp_dir().join(format!("trackmodel-{}", std::process::id()));
        assert!(load(&dir).is_none());
        save(&dir, &groq()).unwrap();
        assert_eq!(load(&dir), Some(groq()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let view = serde_json::to_string(&groq().view()).unwrap();
        assert!(!view.contains("gsk_"), "{view}");
        assert!(view.contains("\"hasKey\":true"), "{view}");
        clear(&dir).unwrap();
        assert!(load(&dir).is_none());
        clear(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `GROQ_API_KEY=… cargo test -- --ignored --nocapture asks_groq_for_settings`
    #[test]
    #[ignore = "needs GROQ_API_KEY and the network"]
    fn asks_groq_for_settings() {
        let key = std::env::var("GROQ_API_KEY").expect("set GROQ_API_KEY");
        let model = TrackModel { key, ..groq() };
        let started = std::time::Instant::now();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let settings = rt
            .block_on(crate::trackllm::ask_settings("a sandy hillside national", &Direct { model }))
            .unwrap();
        println!("{settings:?} in {:.1}s", started.elapsed().as_secs_f32());
        let prog = crate::trackllm::draw_from_settings(&settings, 7).unwrap();
        println!("{} — {:.0} m, {} features", prog.name, prog.lap_length(), prog.features.len());
    }
}
