// Millow — API anahtarlarını macOS Keychain'de güvenli biçimde saklar.

use serde::Serialize;

const KEYCHAIN_SERVICE: &str = "com.millow.app";
const GROQ_ACCOUNT: &str = "groq-api-key";
const GEMINI_ACCOUNT: &str = "gemini-api-key";
const ITEM_NOT_FOUND: i32 = -25300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Groq,
    Gemini,
}

impl SecretKind {
    pub fn parse(provider: &str) -> Result<Self, String> {
        match provider.trim().to_ascii_lowercase().as_str() {
            "groq" => Ok(Self::Groq),
            "gemini" => Ok(Self::Gemini),
            _ => Err("Bilinmeyen API sağlayıcısı".into()),
        }
    }

    fn account(self) -> &'static str {
        match self {
            Self::Groq => GROQ_ACCOUNT,
            Self::Gemini => GEMINI_ACCOUNT,
        }
    }

    pub fn validate(self, value: &str) -> Result<String, String> {
        let value = value.trim();
        if value.is_empty() {
            return Err("API anahtarı boş olamaz".into());
        }

        let valid_prefix = match self {
            Self::Groq => value.starts_with("gsk_"),
            Self::Gemini => value.starts_with("AIza"),
        };
        if !valid_prefix {
            return Err(match self {
                Self::Groq => "Groq anahtarı gsk_ ile başlamalı".into(),
                Self::Gemini => "Gemini anahtarı AIza ile başlamalı".into(),
            });
        }

        Ok(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretStatus {
    pub groq: bool,
    pub gemini: bool,
}

#[cfg(target_os = "macos")]
pub fn get_secret(kind: SecretKind) -> Result<Option<String>, String> {
    use security_framework::passwords::get_generic_password;

    match get_generic_password(KEYCHAIN_SERVICE, kind.account()) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| "Keychain anahtarı geçerli UTF-8 değil".into()),
        Err(error) if error.code() == ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(format!("Keychain okunamadı: {error}")),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn get_secret(_kind: SecretKind) -> Result<Option<String>, String> {
    Err("Güvenli anahtar saklama yalnızca macOS'ta destekleniyor".into())
}

#[cfg(target_os = "macos")]
pub fn set_secret(kind: SecretKind, value: &str) -> Result<(), String> {
    use security_framework::passwords::set_generic_password;

    let value = kind.validate(value)?;
    set_generic_password(KEYCHAIN_SERVICE, kind.account(), value.as_bytes())
        .map_err(|error| format!("Keychain yazılamadı: {error}"))
}

#[cfg(not(target_os = "macos"))]
pub fn set_secret(_kind: SecretKind, _value: &str) -> Result<(), String> {
    Err("Güvenli anahtar saklama yalnızca macOS'ta destekleniyor".into())
}

#[cfg(target_os = "macos")]
pub fn delete_secret(kind: SecretKind) -> Result<(), String> {
    use security_framework::passwords::delete_generic_password;

    match delete_generic_password(KEYCHAIN_SERVICE, kind.account()) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(format!("Keychain kaydı silinemedi: {error}")),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn delete_secret(_kind: SecretKind) -> Result<(), String> {
    Err("Güvenli anahtar saklama yalnızca macOS'ta destekleniyor".into())
}

pub fn secret_status() -> Result<SecretStatus, String> {
    Ok(SecretStatus {
        groq: get_secret(SecretKind::Groq)?.is_some(),
        gemini: get_secret(SecretKind::Gemini)?.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_provider_names_and_key_prefixes() {
        assert_eq!(SecretKind::parse("GROQ").unwrap(), SecretKind::Groq);
        assert!(SecretKind::Groq.validate("gsk_test").is_ok());
        assert!(SecretKind::Groq.validate("AIza_test").is_err());
        assert!(SecretKind::Gemini.validate("AIza_test").is_ok());
        assert!(SecretKind::Gemini.validate("gsk_test").is_err());
    }
}
