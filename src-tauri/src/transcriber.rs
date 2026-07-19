// Millow — Groq Whisper Transkripsiyon (Tek Aşama, Ultra Hızlı)
// Groq Whisper large-v3-turbo ile direkt transcription ~0.5-0.7s
// AI düzeltme YOK — Whisper zaten yeterince iyi

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

static HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

fn get_http_client() -> &'static reqwest::blocking::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}

// ── Groq Whisper API yanıt formatı ──
#[derive(Deserialize)]
struct GroqResponse {
    text: Option<String>,
    segments: Option<Vec<GroqSegment>>,
}

#[derive(Deserialize)]
struct GroqSegment {
    text: String,
    no_speech_prob: f32,
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
    pub last_transcription: Option<String>,
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
            client: get_http_client().clone(),
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

    /// ⚡ Groq Whisper — iki aşamalı transkripsiyon (ASR + Gemini Refinement)
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
            .text("response_format", "verbose_json")
            .text("temperature", "0.0")
            .part("file", reqwest::blocking::multipart::Part::bytes(wav_bytes.to_vec())
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|e| format!("MIME hatası: {}", e))?);

        if let Some(l) = lang {
            form = form.text("language", l.to_string());
            form = form.text("prompt", "dikte, Türkçe, nokta, virgül, yeni satır.");
        } else {
            form = form.text("prompt", "dictation, English, period, comma, new line.");
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

        // Segment bazlı no_speech_prob filtrelemesi
        let raw_text = if let Some(ref segments) = groq_resp.segments {
            let mut filtered_segments = Vec::new();
            for seg in segments {
                if seg.no_speech_prob > 0.5 {
                    println!("🚫 Segment no_speech_prob={:.2} filtrelendi: [{}]", seg.no_speech_prob, seg.text.trim());
                    continue;
                }
                filtered_segments.push(seg.text.trim());
            }
            if filtered_segments.is_empty() {
                String::new()
            } else {
                filtered_segments.join(" ").trim().to_string()
            }
        } else {
            groq_resp.text.unwrap_or_default().trim().to_string()
        };
        
        // Whisper hallucination filtresi — config'den oku
        let cfg_h = crate::config::MillowConfig::load().hallucination_filters;
        let hallucinations: Vec<&str> = cfg_h.iter().map(|s| s.as_str()).collect();
        // Tam eşleşme → tamamen boşalt
        let mut text = if hallucinations.iter().any(|h| raw_text == *h) || raw_text.len() < 3 {
            println!("🚫 Whisper hallucination filtrelendi: [{}]", raw_text);
            String::new()
        } else {
            // Metnin sonundaki/içindeki hallucination'ları temizle
            let mut cleaned = raw_text.clone();
            for h in &hallucinations {
                cleaned = cleaned.replace(h, "");
            }
            cleaned = cleaned.trim().to_string();
            if cleaned != raw_text {
                println!("🧹 Hallucination temizlendi: [{}] → [{}]", raw_text, cleaned);
            }
            cleaned
        };

        // İkinci Aşama: Gemini ile AI post-processing/refinement
        if ctx.ai_editing && !text.is_empty() {
            match self.refine_with_gemini(&text, ctx) {
                Ok(refined) => {
                    text = refined;
                }
                Err(e) => {
                    println!("⚠️ Gemini Refinement başarısız oldu, ham metin kullanılıyor: {}", e);
                }
            }
        }

        let elapsed = t0.elapsed().as_secs_f64();
        println!("⚡ Groq Whisper + Gemini: {:.1}s → \"{}...\"", elapsed,
            &text.chars().take(60).collect::<String>());

        Ok(TranscribeResult {
            result_type: "dictation".into(),
            text,
            action: None,
            params: None,
        })
    }

    /// OpenAI uyumlu chat endpoint'i ile metni düzenler ve temizler
    fn refine_with_gemini(
        &self,
        text: &str,
        ctx: &TranscribeContext,
    ) -> Result<String, String> {
        let prompt = self.build_refinement_prompt(text, ctx);

        #[derive(Serialize)]
        struct OpenAIMessage {
            role: String,
            content: String,
        }

        #[derive(Serialize)]
        struct OpenAIChatRequest {
            model: String,
            messages: Vec<OpenAIMessage>,
        }

        #[derive(Deserialize)]
        struct OpenAIMessageResponse {
            content: Option<String>,
        }

        #[derive(Deserialize)]
        struct OpenAIChoice {
            message: OpenAIMessageResponse,
        }

        #[derive(Deserialize)]
        struct OpenAIChatResponse {
            choices: Option<Vec<OpenAIChoice>>,
        }

        let request = OpenAIChatRequest {
            model: self.model.clone(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let url = format!("{}/v1/chat/completions", self.proxy_endpoint);

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("x-goog-api-key", &self.api_key)
            .json(&request)
            .send()
            .map_err(|e| format!("Refinement API hatası: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(format!("Refinement hatası ({}): {}", status, body));
        }

        let openai_resp: OpenAIChatResponse = response.json()
            .map_err(|e| format!("Refinement yanıt hatası: {}", e))?;

        let refined_text = openai_resp
            .choices
            .and_then(|choices| choices.into_iter().next())
            .map(|choice| choice.message.content.unwrap_or_default())
            .unwrap_or_default()
            .trim()
            .to_string();

        Ok(refined_text)
    }


    /// Gemini düzenleme promptunu oluşturur
    fn build_refinement_prompt(&self, text: &str, ctx: &TranscribeContext) -> String {
        let mut prompt = String::new();
        prompt.push_str("Görevin: macOS dikte asistanı için ses tanımadan gelen ham Türkçe metni düzenlemek.\n");
        prompt.push_str("Kurallar:\n");
        prompt.push_str("- 'ııı, şey, yani, hımm, ee, falan' gibi doldurucu kelimeleri tamamen temizle.\n");
        prompt.push_str("- Yazım, noktalama ve büyük/küçük harf hatalarını düzelt.\n");
        prompt.push_str("- Anlamı koru, akıcı ve doğal hale getir.\n");
        if !ctx.dictionary.is_empty() {
            prompt.push_str(&format!("- Özel terimler/isimler sözlüğü: {}\n", ctx.dictionary.join(", ")));
        }
        prompt.push_str(&format!("- Stil: {}\n", match ctx.writing_style.as_str() {
            "professional" => "Resmi, profesyonel, dilbilgisi kurallarına tam uygun.",
            "casual" => "Günlük konuşma diline uygun, samimi.",
            "technical" => "Teknik ve akademik terimleri bozmayan, net.",
            _ => "Doğal, orijinal konuşma tonunu koruyan."
        }));
        prompt.push_str("- Konuşmacının kendisini düzelttiği kısımları algıla ve sadece düzeltilmiş son anlamlı hali yansıt (örn: 'iki, pardon üçte' -> 'üçte').\n");
        prompt.push_str("- SADECE düzenlenmiş metni döndür. Açıklama, tırnak veya giriş cümlesi ekleme.\n");
        prompt.push_str("- Eğer ham metin boşsa, anlamsızsa veya sadece gürültüden ibaretse SADECE boş bir metin döndür. Önceki cümle bağlamını asla tekrarlama veya buraya kopyalama.\n\n");

        if let Some(ref last) = ctx.last_transcription {
            prompt.push_str(&format!("Önceki cümle bağlamı:\n\"{}\"\n\n", last));
            prompt.push_str("Önceki bağlamı zamirleri ve akışı anlamak için kullan ama yeni metne ekleme.\n\n");
        }

        prompt.push_str("Ham metin:\n");
        prompt.push_str(text);
        
        prompt
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
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("x-goog-api-key", &self.api_key)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_refinement_prompt() {
        let transcriber = GeminiTranscriber::new("key", "http://127.0.0.1:8045", "gemini-3-flash");
        let ctx = TranscribeContext {
            ai_editing: true,
            format_commands: true,
            dictionary: vec!["Millow".to_string(), "Rust".to_string()],
            writing_style: "professional".to_string(),
            active_app: None,
            whisper_mode: false,
            last_transcription: None,
        };
        let prompt = transcriber.build_refinement_prompt("test", &ctx);
        assert!(prompt.contains("Millow, Rust"));
    }
}
