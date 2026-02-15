// Millow — Ayarlar Yönetimi
// Kalıcı ayarları ~/.millow/config.json'dan okur/yazar

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Uygulama ayarları
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MillowConfig {
    /// Gemini API anahtarı (Antigravity proxy)
    pub api_key: String,
    /// Proxy adresi
    pub proxy_endpoint: String,
    /// Gemini model adı
    pub model: String,
    /// Varsayılan dil ("tr" veya "en")
    pub default_language: String,
    /// Çeviri modu aktif mi
    pub translation_enabled: bool,
    /// Çeviri hedef dili
    pub translation_target: String,
    /// Sesli komutlar aktif mi
    pub commands_enabled: bool,
    /// Uyandırma kelimesi aktif mi
    pub wakeword_enabled: bool,
    /// Uyandırma kelimesi
    pub wakeword: String,
    /// Kapatma kelimesi
    pub wakeword_stop: String,
    /// Genel kısayol tuşu
    pub hotkey: String,
    /// Örnekleme hızı (Hz)
    pub sample_rate: u32,

    // ── P1: AI Post-Processing ──
    /// Otomatik AI düzenleme (doldurucu temizleme, gramer, noktalama)
    #[serde(default = "default_true")]
    pub ai_editing: bool,

    // ── P2: Sesli Format Komutları ──
    /// "yeni satır", "nokta" gibi sesli komutları biçime çevir
    #[serde(default = "default_true")]
    pub format_commands: bool,

    // ── P3: Özel Sözlük ──
    /// Kişisel isimler, teknik terimler listesi
    #[serde(default)]
    pub custom_dictionary: Vec<String>,

    // ── P4: Basılı Tutma Modu ──
    /// true ise tuşa basılı tutunca kayıt, bırakınca durdur
    #[serde(default)]
    pub hold_to_talk: bool,

    // ── P5: Stil Eşleştirme ──
    /// Yazım stili: "auto", "professional", "casual", "technical"
    #[serde(default = "default_style")]
    pub writing_style: String,

    // ── P7: Fısıltı Optimizasyonu ──
    /// Düşük sesli/fısıltı konuşma için optimize et
    #[serde(default)]
    pub whisper_mode: bool,

    // ── Groq Whisper (hızlı transcription) ──
    /// Groq API anahtarı (ücretsiz — console.groq.com)
    #[serde(default)]
    pub groq_api_key: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_style() -> String {
    "auto".into()
}

impl Default for MillowConfig {
    fn default() -> Self {
        Self {
            api_key: "sk-e5746968759d4c4cae5a09c32dfc6a6d".into(),
            proxy_endpoint: "http://127.0.0.1:8045".into(),
            model: "gemini-3-flash".into(),
            default_language: "tr".into(),
            translation_enabled: false,
            translation_target: "en".into(),
            commands_enabled: true,
            wakeword_enabled: true,
            wakeword: "millow".into(),
            wakeword_stop: "millow bye bye".into(),
            hotkey: "Option+Space".into(),
            sample_rate: 16000,
            ai_editing: true,
            format_commands: true,
            custom_dictionary: Vec::new(),
            hold_to_talk: true,
            writing_style: "auto".into(),
            whisper_mode: false,
            groq_api_key: None,
        }
    }
}

impl MillowConfig {
    /// Ayarlar dosya yolu
    fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".millow").join("config.json")
    }

    /// Ayarları dosyadan yükle, yoksa varsayılan oluştur
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            let data = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            let config = Self::default();
            config.save();
            config
        }
    }

    /// Ayarları dosyaya kaydet
    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, data);
        }
    }
}
