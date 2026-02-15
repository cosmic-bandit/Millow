// Millow — Sesli Komut Yöneticisi
// Gemini'den gelen komutları macOS'ta çalıştırır

use std::process::Command;

/// Komut çalıştır ve sonucu döndür
pub fn execute_command(action: &str, params: Option<&str>) -> Result<String, String> {
    match action {
        // ── Uygulama Yönetimi ──
        "open_app" => {
            let app_name = params.unwrap_or("Finder");
            Command::new("open")
                .args(["-a", app_name])
                .spawn()
                .map_err(|e| format!("{} açılamadı: {}", app_name, e))?;
            Ok(format!("{} açıldı", app_name))
        }

        // ── Ekran Görüntüsü ──
        "screenshot" => {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let path = format!("{}/Desktop/millow_screenshot_{}.png", home, timestamp);
            Command::new("screencapture")
                .args(["-x", &path])
                .spawn()
                .map_err(|e| format!("Ekran görüntüsü alınamadı: {}", e))?;
            Ok(format!("Ekran görüntüsü kaydedildi: {}", path))
        }

        // ── Ses Kontrolleri ──
        "volume_up" => {
            run_osascript("set volume output volume ((output volume of (get volume settings)) + 10)")?;
            Ok("Ses artırıldı".into())
        }
        "volume_down" => {
            run_osascript("set volume output volume ((output volume of (get volume settings)) - 10)")?;
            Ok("Ses azaltıldı".into())
        }
        "mute" => {
            run_osascript("set volume output muted not (output muted of (get volume settings))")?;
            Ok("Sessiz modu değiştirildi".into())
        }

        // ── Parlaklık ──
        "brightness_up" => {
            // macOS'ta parlaklık kontrolü için brightness komutu
            run_osascript("tell application \"System Events\" to key code 144")?;
            Ok("Parlaklık artırıldı".into())
        }
        "brightness_down" => {
            run_osascript("tell application \"System Events\" to key code 145")?;
            Ok("Parlaklık azaltıldı".into())
        }

        // ── Sistem ──
        "dark_mode" => {
            run_osascript(
                "tell application \"System Events\" to tell appearance preferences to set dark mode to not dark mode",
            )?;
            Ok("Karanlık mod değiştirildi".into())
        }
        "lock_screen" => {
            Command::new("pmset")
                .arg("displaysleepnow")
                .spawn()
                .map_err(|e| format!("Ekran kilitlenemedi: {}", e))?;
            Ok("Ekran kilitlendi".into())
        }
        "wifi_toggle" => {
            // Wi-Fi durumunu kontrol et ve değiştir
            let output = Command::new("networksetup")
                .args(["-getairportpower", "en0"])
                .output()
                .map_err(|e| format!("Wi-Fi kontrolü başarısız: {}", e))?;
            let status = String::from_utf8_lossy(&output.stdout);
            let new_state = if status.contains("On") { "off" } else { "on" };
            Command::new("networksetup")
                .args(["-setairportpower", "en0", new_state])
                .spawn()
                .map_err(|e| format!("Wi-Fi değiştirilemedi: {}", e))?;
            Ok(format!("Wi-Fi {}", if new_state == "on" { "açıldı" } else { "kapatıldı" }))
        }
        "bluetooth_toggle" => {
            run_osascript(
                r#"tell application "System Preferences" to reveal pane id "com.apple.preferences.Bluetooth""#,
            )?;
            Ok("Bluetooth ayarları açıldı".into())
        }

        // ── Medya Kontrol ──
        "play_pause" => {
            run_osascript("tell application \"System Events\" to key code 49 using {}")?;
            // Alternatif: medya tuşu simülasyonu
            Command::new("osascript")
                .args(["-e", "tell application \"Music\" to playpause"])
                .spawn()
                .ok();
            Ok("Oynat/Duraklat".into())
        }
        "next_track" => {
            Command::new("osascript")
                .args(["-e", "tell application \"Music\" to next track"])
                .spawn()
                .map_err(|e| format!("Sonraki şarkı başarısız: {}", e))?;
            Ok("Sonraki şarkı".into())
        }
        "prev_track" => {
            Command::new("osascript")
                .args(["-e", "tell application \"Music\" to previous track"])
                .spawn()
                .map_err(|e| format!("Önceki şarkı başarısız: {}", e))?;
            Ok("Önceki şarkı".into())
        }

        // ── Tarayıcı ──
        "new_tab" => {
            run_osascript(
                r#"tell application "System Events" to keystroke "t" using command down"#,
            )?;
            Ok("Yeni sekme açıldı".into())
        }
        "close_tab" => {
            run_osascript(
                r#"tell application "System Events" to keystroke "w" using command down"#,
            )?;
            Ok("Sekme kapatıldı".into())
        }
        "open_url" => {
            let url = params.unwrap_or("https://google.com");
            Command::new("open")
                .arg(url)
                .spawn()
                .map_err(|e| format!("URL açılamadı: {}", e))?;
            Ok(format!("{} açıldı", url))
        }

        // ── Metin İşlemleri (Kısayol Simülasyonu) ──
        "select_all" => {
            run_osascript(r#"tell application "System Events" to keystroke "a" using command down"#)?;
            Ok("Tümü seçildi".into())
        }
        "copy" => {
            run_osascript(r#"tell application "System Events" to keystroke "c" using command down"#)?;
            Ok("Kopyalandı".into())
        }
        "paste" => {
            run_osascript(r#"tell application "System Events" to keystroke "v" using command down"#)?;
            Ok("Yapıştırıldı".into())
        }
        "undo" => {
            run_osascript(r#"tell application "System Events" to keystroke "z" using command down"#)?;
            Ok("Geri alındı".into())
        }
        "save" => {
            run_osascript(r#"tell application "System Events" to keystroke "s" using command down"#)?;
            Ok("Kaydedildi".into())
        }

        // ── Zamanlayıcı ──
        "set_timer" => {
            let minutes = params.unwrap_or("5");
            let secs: u64 = minutes.parse::<u64>().unwrap_or(5) * 60;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(secs));
                let _ = Command::new("osascript")
                    .args([
                        "-e",
                        &format!(
                            r#"display notification "⏰ {} dakika zamanlayıcı doldu!" with title "Millow""#,
                            secs / 60
                        ),
                    ])
                    .output();
                // Ses çal
                let _ = Command::new("afplay")
                    .arg("/System/Library/Sounds/Glass.aiff")
                    .output();
            });
            Ok(format!("{} dakika zamanlayıcı kuruldu", minutes))
        }

        // ── AI Destekli Panodan İşleme ──
        "translate_clipboard" | "rewrite_clipboard" | "summarize_clipboard" | "generate_code" => {
            // Bu komutlar frontend tarafından özel olarak işlenecek
            // Panodan metin al → Gemini'ye gönder → sonucu panoya koy
            Ok(format!("ai_action:{}", action))
        }

        _ => Err(format!("Bilinmeyen komut: {}", action)),
    }
}

/// AppleScript çalıştır
fn run_osascript(script: &str) -> Result<(), String> {
    Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| format!("AppleScript hatası: {}", e))?;
    Ok(())
}
