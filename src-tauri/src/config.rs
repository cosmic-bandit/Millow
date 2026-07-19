// Millow — Ayarlar Yönetimi
// Kalıcı ayarları ~/.millow/config.json'dan okur/yazar

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Uygulama ayarları
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MillowConfig {
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

    /// Dikte düzenleme seviyesi: "fast", "clean", "rewrite"
    #[serde(default = "default_editing_mode")]
    pub editing_mode: String,

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

    // ── Başlangıçta Çalış ──
    /// Mac açılınca otomatik başlat
    #[serde(default)]
    pub auto_launch: bool,

    // ── Ses & Sessizlik Ayarları ──
    /// Ortam gürültüsü toleransı (0.01-0.50, varsayılan 0.15)
    #[serde(default = "default_noise_tolerance")]
    pub noise_tolerance: f32,

    /// Segment flush sessizlik süresi (saniye, varsayılan 1.5)
    #[serde(default = "default_silence_duration")]
    pub silence_duration: f32,

    /// Otomatik kapanma süresi (saniye, varsayılan 30)
    #[serde(default = "default_auto_stop_duration")]
    pub auto_stop_duration: f32,

    /// Segment flush sonrası satır sonu ekle
    #[serde(default)]
    pub newline_after_segment: bool,

    // ── Hallucination Filtresi ──
    /// Filtrelenen kelimeler/cümleler listesi
    #[serde(default = "default_hallucinations")]
    pub hallucination_filters: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_noise_tolerance() -> f32 {
    0.15
}

fn default_silence_duration() -> f32 {
    1.5
}

fn default_auto_stop_duration() -> f32 {
    30.0
}

fn default_hallucinations() -> Vec<String> {
    vec![
        "Altyazı M.K.".into(),
        "altyazı m.k.".into(),
        "Altyazı M.K".into(),
        "Alt yazı M.K.".into(),
        "Altyazılar M.K.".into(),
        "Altyazı".into(),
        "Alt yazı".into(),
        "Subtitles by".into(),
        "Sottotitoli".into(),
        "Thank you.".into(),
        "Thanks for watching.".into(),
        "Thank you for watching.".into(),
        "you".into(),
        "You".into(),
        "...".into(),
        "…".into(),
        "Teşekkürler.".into(),
        "Teşekkür ederim.".into(),
        "İyi seyirler.".into(),
        "İzlediğiniz için teşekkür ederim.".into(),
        "İzlediğiniz için teşekkürler.".into(),
        "Dinlediğiniz için teşekkürler.".into(),
        "Abone olmayı unutmayın.".into(),
        "Beğenmeyi ve abone olmayı unutmayın.".into(),
        "Please subscribe.".into(),
    ]
}

fn default_style() -> String {
    "auto".into()
}

fn default_editing_mode() -> String {
    "clean".into()
}

fn normalize_model(model: &str) -> (&'static str, bool) {
    match model {
        "gemini-3.5-flash" => ("gemini-3.5-flash", false),
        _ => ("gemini-3.5-flash", true),
    }
}

fn normalize_editing_mode(mode: &str) -> (&'static str, bool) {
    match mode {
        "fast" => ("fast", false),
        "clean" => ("clean", false),
        "rewrite" => ("rewrite", false),
        _ => ("clean", true),
    }
}

impl Default for MillowConfig {
    fn default() -> Self {
        Self {
            model: "gemini-3.5-flash".into(),
            default_language: "tr".into(),
            translation_enabled: false,
            translation_target: "en".into(),
            commands_enabled: true,
            wakeword_enabled: true,
            wakeword: "millow".into(),
            wakeword_stop: "millow bye bye".into(),
            hotkey: "Alt+Space".into(),
            sample_rate: 16000,
            ai_editing: true,
            editing_mode: default_editing_mode(),
            format_commands: true,
            custom_dictionary: Vec::new(),
            hold_to_talk: true,
            writing_style: "auto".into(),
            whisper_mode: false,
            auto_launch: false,
            noise_tolerance: 0.15,
            silence_duration: 1.5,
            auto_stop_duration: 30.0,
            newline_after_segment: false,
            hallucination_filters: default_hallucinations(),
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
            let raw: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            let mut config: Self = serde_json::from_value(raw.clone()).unwrap_or_default();
            let (supported_model, model_migrated) = normalize_model(&config.model);
            if model_migrated {
                config.model = supported_model.into();
            }
            let legacy_hotkey = config.hotkey == "Option+Space";
            if legacy_hotkey {
                config.hotkey = "Alt+Space".into();
            }
            let missing_editing_mode = raw.get("editing_mode").is_none();
            if missing_editing_mode && !config.ai_editing {
                config.editing_mode = "fast".into();
            }
            let (supported_editing_mode, editing_mode_migrated) =
                normalize_editing_mode(&config.editing_mode);
            if editing_mode_migrated {
                config.editing_mode = supported_editing_mode.into();
            }

            // Eski sürümlerde JSON'da tutulan gerçek anahtarları bir kez Keychain'e taşı.
            // Keychain yazımı başarısızsa veri kaybını önlemek için eski dosyayı koru.
            let legacy_fields_present = ["api_key", "proxy_endpoint", "groq_api_key"]
                .iter()
                .any(|key| raw.get(key).is_some());
            let mut safe_to_scrub = true;

            if let Some(groq_key) = raw.get("groq_api_key").and_then(|value| value.as_str()) {
                if groq_key.starts_with("gsk_")
                    && crate::secrets::get_secret(crate::secrets::SecretKind::Groq)
                        .ok()
                        .flatten()
                        .is_none()
                {
                    if let Err(error) =
                        crate::secrets::set_secret(crate::secrets::SecretKind::Groq, groq_key)
                    {
                        eprintln!("Keychain Groq geçişi başarısız: {error}");
                        safe_to_scrub = false;
                    } else {
                        println!("Groq anahtarı macOS Keychain'e taşındı");
                    }
                }
            }

            if let Some(gemini_key) = raw.get("api_key").and_then(|value| value.as_str()) {
                if gemini_key.starts_with("AIza")
                    && crate::secrets::get_secret(crate::secrets::SecretKind::Gemini)
                        .ok()
                        .flatten()
                        .is_none()
                {
                    if let Err(error) =
                        crate::secrets::set_secret(crate::secrets::SecretKind::Gemini, gemini_key)
                    {
                        eprintln!("Keychain Gemini geçişi başarısız: {error}");
                        safe_to_scrub = false;
                    } else {
                        println!("Gemini anahtarı macOS Keychain'e taşındı");
                    }
                }
            }

            if (legacy_fields_present
                || model_migrated
                || legacy_hotkey
                || missing_editing_mode
                || editing_mode_migrated)
                && safe_to_scrub
            {
                config.save();
            }

            config
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_config_never_contains_api_secrets() {
        let value = serde_json::to_value(MillowConfig::default()).unwrap();
        assert!(value.get("api_key").is_none());
        assert!(value.get("groq_api_key").is_none());
        assert!(value.get("proxy_endpoint").is_none());
    }

    #[test]
    fn defaults_use_current_model_and_hotkey_syntax() {
        let config = MillowConfig::default();
        assert_eq!(config.model, "gemini-3.5-flash");
        assert_eq!(config.hotkey, "Alt+Space");
        assert_eq!(config.editing_mode, "clean");
    }

    #[test]
    fn unsupported_config_values_are_normalized() {
        assert_eq!(
            normalize_model("gemini-3.5-flash-low"),
            ("gemini-3.5-flash", true)
        );
        assert_eq!(normalize_editing_mode("invalid"), ("clean", true));
        assert_eq!(normalize_editing_mode("rewrite"), ("rewrite", false));
    }
}
