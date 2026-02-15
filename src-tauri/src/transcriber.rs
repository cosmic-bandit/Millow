// Millow — Groq Whisper Transkripsiyon (Tek Aşama, Ultra Hızlı)
// Groq Whisper large-v3-turbo ile direkt transcription ~0.5-0.7s
// AI düzeltme YOK — Whisper zaten yeterince iyi

use base64::Engine as _;
use serde::{Deserialize, Serialize};

// ── Groq Whisper API yanıt formatı ──
#[derive(Deserialize)]
struct GroqResponse {
    text: Option<String>,
}

// ── Gemini API formatları (fallback) ──
#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
}

#[derive(Serialize)]
struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<Part>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Part {
    Text { text: String },
    InlineData { inline_data: InlineData },
}

#[derive(Serialize)]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Deserialize)]
struct CandidateContent {
    parts: Option<Vec<ResponsePart>>,
}

#[derive(Deserialize)]
struct ResponsePart {
    text: Option<String>,
}

/// Transkripsiyon modu
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscribeMode {
    Dictation,
    Translate { target_lang: String },
    Command,
}

/// Transkripsiyon sonucu
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub result_type: String,
    pub text: String,
    pub action: Option<String>,
    pub params: Option<String>,
}

/// Transkripsiyon bağlamı
#[derive(Debug, Clone, Default)]
pub struct TranscribeContext {
    pub ai_editing: bool,
    pub format_commands: bool,
    pub dictionary: Vec<String>,
    pub writing_style: String,
    pub active_app: Option<String>,
    pub whisper_mode: bool,
}

/// Transkripsiyon motoru
pub struct GeminiTranscriber {
    api_key: String,
    proxy_endpoint: String,
    model: String,
    groq_api_key: Option<String>,
    client: reqwest::blocking::Client,
}

impl GeminiTranscriber {
    pub fn new(api_key: &str, proxy_endpoint: &str, model: &str) -> Self {
        let groq_key = crate::config::MillowConfig::load().groq_api_key;

        Self {
            api_key: api_key.to_string(),
            proxy_endpoint: proxy_endpoint.to_string(),
            model: model.to_string(),
            groq_api_key: groq_key,
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .pool_max_idle_per_host(2)
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new()),
        }
    }

    /// Ana transkripsiyon fonksiyonu
    pub fn transcribe(
        &self,
        wav_bytes: &[u8],
        mode: &TranscribeMode,
        ctx: &TranscribeContext,
    ) -> Result<TranscribeResult, String> {
        if let Some(ref groq_key) = self.groq_api_key {
            if !groq_key.is_empty() {
                return self.groq_transcribe(wav_bytes, mode, ctx, groq_key);
            }
        }
        self.single_stage_gemini(wav_bytes, mode, ctx)
    }

    /// ⚡ Groq Whisper — direkt transcription, AI düzeltme yok
    fn groq_transcribe(
        &self,
        wav_bytes: &[u8],
        mode: &TranscribeMode,
        ctx: &TranscribeContext,
        groq_key: &str,
    ) -> Result<TranscribeResult, String> {
        let t0 = std::time::Instant::now();

        // Çeviri modunda Groq translate endpoint kullan
        let (url, lang) = match mode {
            TranscribeMode::Translate { .. } => {
                ("https://api.groq.com/openai/v1/audio/translations".to_string(), None)
            }
            _ => {
                ("https://api.groq.com/openai/v1/audio/transcriptions".to_string(), Some("tr"))
            }
        };

        let mut form = reqwest::blocking::multipart::Form::new()
            .text("model", "whisper-large-v3-turbo")
            .text("response_format", "json")
            .part("file", reqwest::blocking::multipart::Part::bytes(wav_bytes.to_vec())
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|e| format!("MIME hatası: {}", e))?);

        if let Some(l) = lang {
            form = form.text("language", l.to_string());
        }

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", groq_key))
            .multipart(form)
            .send()
            .map_err(|e| format!("Groq hatası: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(format!("Groq hatası ({}): {}", status, body));
        }

        let groq_resp: GroqResponse = response.json()
            .map_err(|e| format!("Groq JSON hatası: {}", e))?;

        let text = groq_resp.text.unwrap_or_default().trim().to_string();
        let elapsed = t0.elapsed().as_secs_f64();
        println!("⚡ Groq Whisper: {:.1}s → \"{}...\"", elapsed,
            &text.chars().take(60).collect::<String>());

        Ok(TranscribeResult {
            result_type: "dictation".into(),
            text,
            action: None,
            params: None,
        })
    }

    /// Tek aşamalı Gemini (fallback — Groq key yoksa)
    fn single_stage_gemini(
        &self,
        wav_bytes: &[u8],
        mode: &TranscribeMode,
        ctx: &TranscribeContext,
    ) -> Result<TranscribeResult, String> {
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(wav_bytes);

        let prompt = match mode {
            TranscribeMode::Dictation => self.build_dictation_prompt(ctx),
            TranscribeMode::Translate { target_lang } => {
                format!("Transkript et ve {} diline çevir. SADECE sonucu döndür.", target_lang)
            }
            TranscribeMode::Command => {
                r#"Sesi analiz et. SADECE JSON döndür:{"result_type":"dictation"|"command"|"wakeword"|"sleep","text":"...","action":"...","params":"..."}"#.to_string()
            }
        };

        let request = GeminiRequest {
            contents: vec![Content {
                role: Some("user".to_string()),
                parts: vec![
                    Part::Text { text: prompt },
                    Part::InlineData {
                        inline_data: InlineData {
                            mime_type: "audio/wav".into(),
                            data: audio_b64,
                        },
                    },
                ],
            }],
        };

        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.proxy_endpoint, self.model, self.api_key
        );

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| format!("API hatası: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(format!("API hatası ({}): {}", status, body));
        }

        let gemini_resp: GeminiResponse = response.json()
            .map_err(|e| format!("Yanıt hatası: {}", e))?;

        let text = gemini_resp
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|c| c.parts)
            .and_then(|p| p.into_iter().next())
            .and_then(|p| p.text)
            .unwrap_or_default()
            .trim()
            .to_string();

        if matches!(mode, TranscribeMode::Command) {
            if let Ok(result) = serde_json::from_str::<TranscribeResult>(&text) {
                return Ok(result);
            }
            let cleaned = text
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();
            if let Ok(result) = serde_json::from_str::<TranscribeResult>(cleaned) {
                return Ok(result);
            }
        }

        Ok(TranscribeResult {
            result_type: "dictation".into(),
            text,
            action: None,
            params: None,
        })
    }

    fn build_dictation_prompt(&self, ctx: &TranscribeContext) -> String {
        let mut prompt = String::from("Metni transkript et. ");
        if ctx.ai_editing {
            prompt.push_str("Doldurucuları temizle. Gramer ve noktalamayı düzelt. ");
        }
        if ctx.format_commands {
            prompt.push_str("Sesli komutları uygula. ");
        }
        if !ctx.dictionary.is_empty() {
            prompt.push_str(&format!("Terimler: {}. ", ctx.dictionary.join(", ")));
        }
        prompt.push_str(&format!("Üslup: {}. SADECE metni döndür.", ctx.writing_style));
        prompt
    }
}
