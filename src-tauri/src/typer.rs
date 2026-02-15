// Millow — Otomatik Yazıcı
// pbcopy + CGEvent Cmd+V ile aktif uygulamaya metin yapıştırma

use std::process::Command;

/// Otomatik metin yazıcı
pub struct AutoTyper;

impl AutoTyper {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }

    /// Metni belirtilen uygulamaya yapıştır
    /// target_app: kayıt başlamadan önceki aktif uygulama adı
    pub fn type_text(&self, text: &str) -> Result<(), String> {
        self.type_text_to_app(text, None)
    }

    /// Metni belirtilen uygulamaya yapıştır
    pub fn type_text_to_app(&self, text: &str, target_app: Option<&str>) -> Result<(), String> {
        // 1. Mevcut clipboard'ı yedekle
        let old_clipboard = Command::new("pbpaste")
            .env("LANG", "en_US.UTF-8")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok());
        
        println!("⌨️ AutoTyper: Yazılıyor (hedef: {:?})", target_app);

        // 2. Metni clipboard'a kopyala
        let mut child = Command::new("pbcopy")
            .env("LANG", "en_US.UTF-8")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("pbcopy hatası: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())
                .map_err(|e| format!("Clipboard yazma hatası: {}", e))?;
        }
        child.wait().map_err(|e| format!("pbcopy bekleme hatası: {}", e))?;

        // 3. Hedef uygulamaya focus ver
        if let Some(app_name) = target_app {
            println!("🔄 Focus veriliyor: {}", app_name);
            let script = format!(
                "tell application \"{}\" to activate",
                app_name.replace('"', "\\\"")
            );
            let _ = Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output();
            // Uygulama focus alana kadar bekle
            std::thread::sleep(std::time::Duration::from_millis(300));
        }

        // 4. CGEvent ile Cmd+V yapıştır
        unsafe {
            use core_graphics::event::{CGEvent, CGEventFlags, CGKeyCode};
            use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

            let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| "CGEventSource oluşturulamadı".to_string())?;

            // V tuşu keycode = 9
            let key_down = CGEvent::new_keyboard_event(source.clone(), 9, true)
                .map_err(|_| "KeyDown event oluşturulamadı".to_string())?;
            let key_up = CGEvent::new_keyboard_event(source, 9, false)
                .map_err(|_| "KeyUp event oluşturulamadı".to_string())?;

            key_down.set_flags(CGEventFlags::CGEventFlagCommand);
            key_up.set_flags(CGEventFlags::CGEventFlagCommand);

            key_down.post(core_graphics::event::CGEventTapLocation::HID);
            key_up.post(core_graphics::event::CGEventTapLocation::HID);
        }

        // 5. Yapıştırma bekleme
        let wait_ms = if target_app.is_some() { 250 } else { 150 };
        std::thread::sleep(std::time::Duration::from_millis(wait_ms));

        // 6. Eski clipboard'ı geri yükle
        if let Some(old) = old_clipboard {
            if let Ok(mut restore) = Command::new("pbcopy")
                .env("LANG", "en_US.UTF-8")
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = restore.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(old.as_bytes());
                }
                let _ = restore.wait();
            }
        }

        Ok(())
    }
}
