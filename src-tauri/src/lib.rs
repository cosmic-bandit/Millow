// Millow — Ana Uygulama Modülü
// Tüm modülleri birleştirir, tray menü ve global kısayolu Rust tarafında yönetir

mod audio;
mod commander;
mod config;
mod transcriber;
mod typer;

use audio::AudioEngine;
use config::MillowConfig;
use parking_lot::Mutex;
use std::sync::OnceLock;
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

use std::sync::Arc;
use tauri::{
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Manager, WebviewWindow,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

// macOS Dock gizleme/gösterme
#[cfg(target_os = "macos")]
use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy};

/// Dock'ta görünür yap
#[cfg(target_os = "macos")]
fn show_dock() {
    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular);
    }
}

/// Dock'tan gizle (sadece menü bar)
#[cfg(target_os = "macos")]
fn hide_dock() {
    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory);
    }
}

use transcriber::{GeminiTranscriber, TranscribeContext, TranscribeMode};

/// Uygulama durumu
pub struct AppState {
    audio_engine: Mutex<AudioEngine>,
    config: Mutex<MillowConfig>,
    /// Uygulama aktif mi (uyandırma kelimesiyle kontrol)
    is_active: Mutex<bool>,
    /// Mevcut mod: "dictation", "translate", "command"
    current_mode: Mutex<String>,
    /// Kayıt başladığında aktif olan uygulama
    source_app: Mutex<Option<String>>,
    /// Kayıt durumu
    is_recording: Mutex<bool>,
    is_processing: std::sync::atomic::AtomicBool,
    /// Pencere görünür mü (rdev crash fix)
    window_visible: std::sync::atomic::AtomicBool,
    /// Debounce: son kayıt başlama zamanı
    last_record_start: Mutex<std::time::Instant>,
}

/// P6: macOS'ta aktif uygulamanın adını al
fn get_active_app() -> Option<String> {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events" to get name of first application process whose frontmost is true"#,
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// Config'den TranscribeContext oluştur
fn build_context(config: &MillowConfig) -> TranscribeContext {
    TranscribeContext {
        ai_editing: config.ai_editing,
        format_commands: config.format_commands,
        dictionary: config.custom_dictionary.clone(),
        writing_style: config.writing_style.clone(),
        active_app: get_active_app(),
        whisper_mode: config.whisper_mode,
    }
}

/// Segment flush: mevcut buffer'ı transkript edip yapıştır, kayda devam et
pub fn flush_segment(state: Arc<AppState>) {
    use std::sync::atomic::Ordering;
    if state.is_processing.load(Ordering::SeqCst) {
        println!("⚠️  Zaten işleniyor, segment flush atlanıyor");
        return;
    }
    
    let samples = state.audio_engine.lock().drain_samples();
    if samples.is_empty() {
        println!("⏭️  Segment boş, atlanıyor");
        return;
    }
    
    // Ses seviyesi kontrolü — sessiz segmentleri atla (API hallucination önleme)
    let rms: f64 = (samples.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / samples.len() as f64).sqrt();
    let peak = samples.iter().map(|s| s.abs() as u16).max().unwrap_or(0);
    if rms < 200.0 && peak < 400 {
        println!("⏭️  Segment çok sessiz (rms={:.0}, peak={}), atlanıyor", rms, peak);
        return;
    }
    
    state.is_processing.store(true, Ordering::SeqCst);
    
    let config = state.config.lock().clone();
    let actual_rate = state.audio_engine.lock().get_actual_sample_rate();
    let wav_bytes = match AudioEngine::samples_to_wav(&samples, actual_rate) {
        Ok(b) => b,
        Err(e) => {
            println!("❌ Segment WAV hatası: {}", e);
            state.is_processing.store(false, Ordering::SeqCst);
            return;
        }
    };
    
    let duration = samples.len() as f32 / config.sample_rate as f32;
    println!("📝 Segment flush: {:.1}s ses transkript ediliyor…", duration);
    
    let mode = {
        let current = state.current_mode.lock().clone();
        match current.as_str() {
            "translate" => TranscribeMode::Translate {
                target_lang: config.translation_target.clone(),
            },
            "command" => TranscribeMode::Command,
            _ => TranscribeMode::Dictation,
        }
    };
    
    let ctx = build_context(&config);
    let transcriber = Arc::new(GeminiTranscriber::new(
        &config.api_key,
        &config.proxy_endpoint,
        &config.model,
    ));
    
    let state_proc = Arc::clone(&state);
    std::thread::spawn(move || {
        let t_start = std::time::Instant::now();
        match transcriber.transcribe(&wav_bytes, &mode, &ctx) {
            Ok(result) => {
                println!("📝 Segment sonuç ({:.1}s): {:?}", t_start.elapsed().as_secs_f64(), result);
                if !result.text.is_empty() {
                    match typer::AutoTyper::new() {
                        Ok(t) => {
                            let src_app = state_proc.source_app.lock().clone();
                            if let Err(e) = t.type_text_to_app(&result.text, src_app.as_deref()) {
                                println!("❌ Segment yazma hatası: {}", e);
                            } else {
                                println!("✅ Segment yazıldı: {}", result.text);
                            }
                        }
                        Err(e) => println!("❌ AutoTyper hatası: {}", e),
                    }
                }
            }
            Err(e) => {
                println!("❌ Segment transkript hatası: {}", e);
            }
        }
        state_proc.is_processing.store(false, Ordering::SeqCst);
    });
}

/// Kaydı başlat/durdur ve transkript et (Rust tarafında tam döngü)
pub fn toggle_recording(state: Arc<AppState>) {
    use std::sync::atomic::Ordering;
    if state.is_processing.load(Ordering::SeqCst) {
        println!("⚠️  Zaten isleniyor, atlaniyor");
        return;
    }
    let is_rec = *state.is_recording.lock();
    println!("⏺️  toggle_recording çağrıldı (is_recording: {})", is_rec);

    if is_rec {
        // ── Kaydı durdur & transkript et ──
        println!("⏹️  Kayıt durduruluyor…");
        state.is_processing.store(true, Ordering::SeqCst);
        *state.is_recording.lock() = false;

        let samples = state.audio_engine.lock().stop_recording();
        if samples.is_empty() {
            println!("❌ Ses kaydı boş");
            notify("Ses kaydı boş", "Mikrofona konuştuğunuzdan emin olun");
            state.is_processing.store(false, Ordering::SeqCst);
            return;
        }

        let config = state.config.lock().clone();
        let actual_rate = state.audio_engine.lock().get_actual_sample_rate();
        let wav_bytes = match AudioEngine::samples_to_wav(&samples, actual_rate) {
            Ok(b) => b,
            Err(e) => {
                println!("❌ WAV dönüşüm hatası: {}", e);
                state.is_processing.store(false, Ordering::SeqCst);
                notify("Hata", &e);
                return;
            }
        };

        let duration = samples.len() as f32 / config.sample_rate as f32;
        println!("✅ {} saniye ses kaydedildi, transkript ediliyor…", duration);
        notify("İşleniyor…", &format!("{:.1}s ses transkript ediliyor", duration));

        // Mod belirle
        let mode = {
            let current = state.current_mode.lock().clone();
            match current.as_str() {
                "translate" => TranscribeMode::Translate {
                    target_lang: config.translation_target.clone(),
                },
                "command" => TranscribeMode::Command,
                _ => {
                    if false {
                        TranscribeMode::Command
                    } else {
                        TranscribeMode::Dictation
                    }
                }
            }
        };

        // P1-P7: Bağlam oluştur
        let ctx = build_context(&config);

        let transcriber = Arc::new(GeminiTranscriber::new(
            &config.api_key,
            &config.proxy_endpoint,
            &config.model,
        ));

        let state_internal = Arc::clone(&state);
        let state_proc = Arc::clone(&state);
        std::thread::spawn(move || {
            let t_start = std::time::Instant::now();
            match transcriber.transcribe(&wav_bytes, &mode, &ctx) {
                Ok(result) => {
                    println!("📝 Sonuç ({:.1}s): {:?}", t_start.elapsed().as_secs_f64(), result);
                    match result.result_type.as_str() {
                        "dictation" => {
                            if !result.text.is_empty() {
                                match typer::AutoTyper::new() {
                                    Ok(t) => {
                                        let src_app = state_internal.source_app.lock().clone();
                                        if let Err(e) =
                                            t.type_text_to_app(&result.text, src_app.as_deref())
                                        {
                                            println!("❌ Yazma hatası: {}", e);
                                            notify("Yazma hatası", &e);
                                        } else {
                                            println!("✅ Yazıldı: {}", result.text);
                                            notify("✅ Yazıldı", &result.text);
                                        }
                                    }
                                    Err(e) => {
                                        println!("❌ Typer hatası: {}", e);
                                        notify("Typer hatası", &e);
                                    }
                                }
                            }
                        }
                        "command" => {
                            if let Some(ref action) = result.action {
                                match commander::execute_command(action, result.params.as_deref()) {
                                    Ok(msg) => {
                                        println!("✅ Komut: {} → {}", action, msg);
                                        notify("Komut çalıştırıldı", &msg);
                                    }
                                    Err(e) => {
                                        println!("❌ Komut hatası: {}", e);
                                        notify("Komut hatası", &e);
                                    }
                                }
                            }
                        }
                        "wakeword" => {
                            *state_internal.is_active.lock() = true;
                            println!("🌿 Millow aktif!");
                            notify("🌿 Millow", "Aktif — dinliyorum!");
                        }
                        "sleep" => {
                            *state_internal.is_active.lock() = false;
                            println!("😴 Millow uyuyor");
                            notify("😴 Millow", "Uyku moduna geçildi");
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    println!("❌ Transkripsiyon hatası: {}", e);
                    notify("Transkripsiyon hatası", &e);
                }
            }
            state_proc.is_processing.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    } else {
        // ── Kaydı başlat ──
        match state.audio_engine.lock().start_recording() {
            Ok(_) => {
                // Kayıt başlamadan önceki aktif uygulamayı kaydet
                *state.source_app.lock() = get_active_app();
                *state.is_recording.lock() = true;
                println!("🎙️  Kayıt başladı!");
                std::thread::spawn(|| { notify("🎙️ Kayıt", "3s susunca yazar, 30s susunca kapanır"); });
                
                // Watchdog: 3s sessizlik → segment flush, 30s → kapat
                let state_wd = Arc::clone(&state);
                std::thread::spawn(move || {
                    let mut total_silence: f64 = 0.0;
                    let mut had_voice = false;
                    let mut segment_flushed = false;
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        let is_rec = *state_wd.is_recording.lock();
                        if !is_rec { break; }
                        
                        let silence_secs = state_wd.audio_engine.lock().seconds_since_voice();
                        
                        if silence_secs < 1.0 {
                            had_voice = true;
                            segment_flushed = false;
                            total_silence = 0.0;
                        } else {
                            total_silence = silence_secs;
                        }
                        
                        if had_voice && !segment_flushed && silence_secs >= 1.5 {
                            println!("📝 3s sessizlik — segment flush");
                            flush_segment(Arc::clone(&state_wd));
                            segment_flushed = true;
                            had_voice = false;
                        }
                        
                        if total_silence >= 30.0 {
                            println!("🔇 30s sessizlik — otomatik durdurma");
                            notify("🔇 Sessizlik", "30s ses gelmedi, durduruldu");
                            toggle_recording(Arc::clone(&state_wd));
                            break;
                        }
                    }
                });
            }
            Err(e) => {
                let err_msg = e.to_string();
                println!("❌ Kayıt başlatılamadı: {}", err_msg);
                notify("Mikrofon hatası", &err_msg);
            }
        }
    }
}

/// macOS bildirimi göster
fn notify(title: &str, message: &str) {
    if let Some(handle) = APP_HANDLE.get() {
        use tauri_plugin_notification::NotificationExt;
        let _ = handle.notification()
            .builder()
            .title(title)
            .body(message)
            .show();
        return;
    }
    let _ = std::process::Command::new("osascript")
        .args(["-e", &format!(
            "display notification \"{}\" with title \"{}\"",
            message.replace('"', "'"), title.replace('"', "'")
        )])
        .output();
}

#[tauri::command]
fn start_recording(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    state.audio_engine.lock().start_recording()?;
    *state.source_app.lock() = get_active_app();
    *state.is_recording.lock() = true;
    Ok("Kayıt başladı".into())
}

#[tauri::command]
async fn stop_and_transcribe(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    *state.is_recording.lock() = false;

    let wav_bytes = {
        let mut audio = state.audio_engine.lock();
        let samples = audio.stop_recording();
        if samples.is_empty() {
            return Err("Ses kaydı boş".into());
        }
        let actual_rate = audio.get_actual_sample_rate();
        AudioEngine::samples_to_wav(&samples, actual_rate)?
    }; // audio kilidi burada (await öncesinde) serbest bırakılır

    let config = state.config.lock().clone();
    let transcriber = GeminiTranscriber::new(&config.api_key, &config.proxy_endpoint, &config.model);
    let mode = if false {
        TranscribeMode::Command
    } else {
        TranscribeMode::Dictation
    };
    let ctx = build_context(&config);
    let result = transcriber.transcribe(&wav_bytes, &mode, &ctx)?;
    Ok(serde_json::to_value(&result).unwrap_or_default())
}

#[tauri::command]
fn is_recording_cmd(state: tauri::State<'_, Arc<AppState>>) -> bool {
    *state.is_recording.lock()
}

#[tauri::command]
fn get_config(state: tauri::State<'_, Arc<AppState>>) -> MillowConfig {
    state.config.lock().clone()
}

#[tauri::command]
fn save_config(state: tauri::State<'_, Arc<AppState>>, new_config: MillowConfig) {
    let mut config = state.config.lock();
    *config = new_config.clone();
    new_config.save();
}

#[tauri::command]
fn set_mode(state: tauri::State<'_, Arc<AppState>>, mode: String) {
    *state.current_mode.lock() = mode;
}

#[tauri::command]
fn health_check() -> String {
    "Millow çalışıyor 🌿".into()
}

#[tauri::command]
fn change_hotkey(app: AppHandle, state: tauri::State<'_, Arc<AppState>>, new_hotkey: String) -> Result<String, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    
    // Eski kısayolu kaldır
    let old_hotkey = state.config.lock().hotkey.clone();
    if old_hotkey != "FnDoubleTap" {
        let _ = app.global_shortcut().unregister(old_hotkey.as_str());
    }
    
    // FnDoubleTap seçildiyse global shortcut kaydetme, rdev halleder
    if new_hotkey == "FnDoubleTap" {
        state.config.lock().hotkey = new_hotkey.clone();
        state.config.lock().save();
        println!("🎹 Kısayol değiştirildi: {} → {} (rdev)", old_hotkey, new_hotkey);
        return Ok(format!("Kısayol değiştirildi: {}", new_hotkey));
    }
    
    // Yeni kısayolu kaydet
    let state_clone = (*state).clone();
    app.global_shortcut().on_shortcut(new_hotkey.as_str(), move |_app, _shortcut, event| {
        let hold_mode = state_clone.config.lock().hold_to_talk;
        if hold_mode {
            match event.state {
                tauri_plugin_global_shortcut::ShortcutState::Pressed => {
                    let is_rec = *state_clone.is_recording.lock();
                    let elapsed = state_clone.last_record_start.lock().elapsed();
                    if !is_rec && elapsed.as_millis() > 500 {
                        *state_clone.last_record_start.lock() = std::time::Instant::now();
                        let state = state_clone.clone();
                        std::thread::spawn(move || {
                            match state.audio_engine.lock().start_recording() {
                                Ok(_) => {
                                    *state.source_app.lock() = get_active_app();
                                    *state.is_recording.lock() = true;
                                    println!("🎙️  Kayıt başladı (basılı tutma)");
                                }
                                Err(e) => println!("❌ Kayıt hatası: {}", e),
                            }
                        });
                    }
                }
                tauri_plugin_global_shortcut::ShortcutState::Released => {
                    let is_rec = *state_clone.is_recording.lock();
                    if is_rec {
                        let state = state_clone.clone();
                        std::thread::spawn(move || {
                            toggle_recording(state);
                        });
                    }
                }
            }
        } else {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let state = state_clone.clone();
                std::thread::spawn(move || {
                    toggle_recording(state);
                });
            }
        }
    }).map_err(|e| format!("Kısayol hatası: {}", e))?;
    
    // Config güncelle
    state.config.lock().hotkey = new_hotkey.clone();
    state.config.lock().save();
    
    println!("🎹 Kısayol değiştirildi: {} → {}", old_hotkey, new_hotkey);
    Ok(format!("Kısayol değiştirildi: {}", new_hotkey))
}

// ── Başlangıçta Çalış (LaunchAgent) ──

fn launch_agent_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.millow.app.plist")
}

fn get_app_path() -> String {
    if std::path::Path::new("/Applications/Millow.app").exists() {
        "/Applications/Millow.app".to_string()
    } else {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

#[tauri::command]
fn get_auto_launch() -> bool {
    launch_agent_path().exists()
}

#[tauri::command]
fn set_auto_launch(state: tauri::State<'_, Arc<AppState>>, enabled: bool) -> Result<String, String> {
    let plist_path = launch_agent_path();

    if enabled {
        let app_path = get_app_path();
        if app_path.is_empty() {
            return Err("Uygulama yolu bulunamadı".into());
        }

        let plist_content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n    <key>Label</key>\n    <string>com.millow.app</string>\n    <key>ProgramArguments</key>\n    <array>\n        <string>/usr/bin/open</string>\n        <string>-a</string>\n        <string>{}</string>\n    </array>\n    <key>RunAtLoad</key>\n    <true/>\n</dict>\n</plist>",
            app_path
        );

        if let Some(parent) = plist_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        std::fs::write(&plist_path, plist_content)
            .map_err(|e| format!("LaunchAgent yazılamadı: {}", e))?;

        println!("✅ Başlangıçta çalış aktif: {}", plist_path.display());
    } else {
        if plist_path.exists() {
            std::fs::remove_file(&plist_path)
                .map_err(|e| format!("LaunchAgent silinemedi: {}", e))?;
        }
        println!("❌ Başlangıçta çalış devre dışı");
    }

    let mut config = state.config.lock();
    config.auto_launch = enabled;
    config.save();

    Ok(if enabled { "Aktif".into() } else { "Devre dışı".into() })
}

// ── Uygulama Başlatma ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = MillowConfig::load();
    let sample_rate = config.sample_rate;

    let app_state = Arc::new(AppState {
        audio_engine: Mutex::new(AudioEngine::new(sample_rate)),
        config: Mutex::new(config),
        is_active: Mutex::new(false),
        current_mode: Mutex::new("dictation".into()),
        source_app: Mutex::new(None),
        is_recording: Mutex::new(false),
        is_processing: std::sync::atomic::AtomicBool::new(false),
        window_visible: std::sync::atomic::AtomicBool::new(false),
        last_record_start: Mutex::new(std::time::Instant::now()),
    });

    let state_for_manager = app_state.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_and_transcribe,
            is_recording_cmd,
            get_config,
            save_config,
            set_mode,
            health_check,
            change_hotkey,
            get_auto_launch,
            set_auto_launch,
        ])
        .setup(move |app| {
            let _ = APP_HANDLE.set(app.handle().clone());
            
            // Bildirim izni iste
            {
                use tauri_plugin_notification::NotificationExt;
                let _ = app.notification().request_permission();
            }
            // ── Menü Oluştur ──
            let toggle = MenuItemBuilder::with_id("toggle", "Kayıt Başlat/Durdur")
                .build(app)?;
            let mode_dictation =
                MenuItemBuilder::with_id("mode_dictation", "Yazdırma Modu").build(app)?;
            let mode_translate =
                MenuItemBuilder::with_id("mode_translate", "Çeviri Modu").build(app)?;
            let mode_command =
                MenuItemBuilder::with_id("mode_command", "Komut Modu").build(app)?;
            let sep1 = MenuItemBuilder::with_id("sep1", "───────────")
                .enabled(false)
                .build(app)?;
            let sep2 = MenuItemBuilder::with_id("sep2", "───────────")
                .enabled(false)
                .build(app)?;
            let settings = MenuItemBuilder::with_id("settings", "Ayarlar…").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Çıkış").build(app)?;

            let menu = MenuBuilder::new(app)
                .items(&[
                    &toggle,
                    &sep1,
                    &mode_dictation,
                    &mode_translate,
                    &mode_command,
                    &sep2,
                    &settings,
                    &quit,
                ])
                .build()?;

            // ── Tray İkonu ──
            let state_for_tray = state_for_manager.clone();
            let _tray = TrayIconBuilder::new()
                .icon(
                    tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
                        .expect("tray ikon yüklenemedi"),
                )
                .icon_as_template(false) // Renkli logo göster
                .menu(&menu)
                .menu_on_left_click(true)
                .on_menu_event(move |app: &AppHandle, event: MenuEvent| {
                    match event.id().as_ref() {
                        "toggle" => {
                            let state = state_for_tray.clone();
                            std::thread::spawn(move || {
            let t_start = std::time::Instant::now();
                                toggle_recording(state);
                            });
                        }
                        "mode_dictation" => {
                            *state_for_tray.current_mode.lock() = "dictation".into();
                            notify("Mod", "📝 Yazdırma modu aktif");
                        }
                        "mode_translate" => {
                            *state_for_tray.current_mode.lock() = "translate".into();
                            notify("Mod", "🌍 Çeviri modu aktif");
                        }
                        "mode_command" => {
                            *state_for_tray.current_mode.lock() = "command".into();
                            notify("Mod", "🤖 Komut modu aktif");
                        }
                        "settings" => {
                            state_for_tray.window_visible.store(true, std::sync::atomic::Ordering::Relaxed);
                            #[cfg(target_os = "macos")]
                            show_dock();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show().unwrap();
                                let _ = window.set_focus().unwrap();
                            }
                        }
                        "quit" => {
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            // ── P4: Global Kısayol — hold_to_talk destekli ──
            let state_for_shortcut = state_for_manager.clone();
            let hotkey_str = state_for_manager.config.lock().hotkey.clone();
            println!("🎹 Kısayol: {}", hotkey_str);
            app.global_shortcut().on_shortcut(hotkey_str.as_str(), move |_app, _shortcut, event| {
                let hold_mode = state_for_shortcut.config.lock().hold_to_talk;

                if hold_mode {
                    // P4: Basılı tutma modu — basınca kayıt, bırakınca durdur
                    match event.state {
                        tauri_plugin_global_shortcut::ShortcutState::Pressed => {
                            let is_rec = *state_for_shortcut.is_recording.lock();
                            // Debounce: 500ms içinde tekrar tetiklenmeyi engelle
                            let elapsed = state_for_shortcut.last_record_start.lock().elapsed();
                            if !is_rec && elapsed.as_millis() > 500 {
                                *state_for_shortcut.last_record_start.lock() = std::time::Instant::now();
                                let state = state_for_shortcut.clone();
                                std::thread::spawn(move || {
            let t_start = std::time::Instant::now();
                                    match state.audio_engine.lock().start_recording() {
                                        Ok(_) => {
                                            // Kayıt başlamadan önceki aktif uygulamayı kaydet
                *state.source_app.lock() = get_active_app();
                *state.is_recording.lock() = true;
                                            println!("🎙️  Kayıt başladı (basılı tutma)");
                                        }
                                        Err(e) => println!("❌ Kayıt hatası: {}", e),
                                    }
                                });
                            }
                        }
                        tauri_plugin_global_shortcut::ShortcutState::Released => {
                            let is_rec = *state_for_shortcut.is_recording.lock();
                            if is_rec {
                                let state = state_for_shortcut.clone();
                                std::thread::spawn(move || {
            let t_start = std::time::Instant::now();
                                    toggle_recording(state);
                                });
                            }
                        }
                    }
                } else {
                    // Normal toggle modu
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let state = state_for_shortcut.clone();
                        std::thread::spawn(move || {
            let t_start = std::time::Instant::now();
                            toggle_recording(state);
                        });
                    }
                }
            })?;

            // ── Double-Tap Fn Tuşu Dinleyicisi (NSEvent global monitor) ──
            let state_for_fn = state_for_manager.clone();
            // NSEvent.addGlobalMonitorForEvents — main-thread-safe, WKWebView ile çakışmaz
            {
                use cocoa::base::{id, nil};
                use cocoa::foundation::NSAutoreleasePool;
                use objc::runtime::Object;
                use objc::msg_send;
                use objc::sel;
                use objc::sel_impl;
                use std::sync::Arc;
                
                let state = state_for_fn.clone();
                let last_fn_press = Arc::new(parking_lot::Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(10)));
                
                let last_fn = last_fn_press.clone();
                let state_cb = state.clone();
                
                // NSEvent flagsChanged mask = 1 << 12 = 4096 = NSEventMaskFlagsChanged
                let mask: u64 = 1 << 12; // NSEventMaskFlagsChanged
                
                let block = block::ConcreteBlock::new(move |event: id| {
                    // Pencere açıkken ignore et
                    if state_cb.window_visible.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }
                    
                    unsafe {
                        let flags: u64 = msg_send![event, modifierFlags];
                        let fn_flag: u64 = 1 << 23; // NSEventModifierFlagFunction = 0x800000
                        
                        if flags & fn_flag != 0 {
                            let now = std::time::Instant::now();
                            let mut last = last_fn.lock();
                            let elapsed = now.duration_since(*last);
                            
                            if elapsed.as_millis() < 400 && elapsed.as_millis() > 50 {
                                println!("🎹 Double-tap Fn algılandı! ({:.0}ms)", elapsed.as_millis());
                                *last = now - std::time::Duration::from_secs(10);
                                
                                let is_rec = *state_cb.is_recording.lock();
                                if !is_rec {
                                    let state_start = Arc::clone(&state_cb);
                                    std::thread::spawn(move || {
                                        match state_start.audio_engine.lock().start_recording() {
                                            Ok(_) => {
                                                *state_start.source_app.lock() = get_active_app();
                                                *state_start.is_recording.lock() = true;
                                                println!("🎙️  Fn kayıt başladı (hedef: {:?})", state_start.source_app.lock());
                                                std::thread::spawn(|| { notify("🎙️ Kayıt", "3s susunca yazar, 30s susunca kapanır"); });
                                                // Watchdog: 3s sessizlik → segment flush, 30s → kapat
                                                let state_wd = Arc::clone(&state_start);
                                                std::thread::spawn(move || {
                                                    let mut total_silence: f64 = 0.0;
                                                    let mut had_voice = false; // Hiç konuşma oldu mu
                                                    let mut segment_flushed = false; // Bu segment flush edildi mi
                                                    loop {
                                                        std::thread::sleep(std::time::Duration::from_millis(500));
                                                        let is_rec = *state_wd.is_recording.lock();
                                                        if !is_rec { break; }
                                                        
                                                        let silence_secs = state_wd.audio_engine.lock().seconds_since_voice();
                                                        
                                                        if silence_secs < 1.0 {
                                                            // Konuşma var
                                                            had_voice = true;
                                                            segment_flushed = false;
                                                            total_silence = 0.0;
                                                        } else {
                                                            total_silence = silence_secs;
                                                        }
                                                        
                                                        // 3s sessizlik + konuşma olduysa → segment flush

                                                        if had_voice && !segment_flushed && silence_secs >= 1.5 {
                                                            println!("📝 3s sessizlik — segment flush");
                                                            flush_segment(Arc::clone(&state_wd));
                                                            segment_flushed = true;
                                                            had_voice = false;
                                                        }
                                                        
                                                        // 30s toplam sessizlik → tamamen kapat
                                                        if total_silence >= 30.0 {
                                                            println!("🔇 30s sessizlik — otomatik durdurma");
                                                            notify("🔇 Sessizlik", "30s ses gelmedi, durduruldu");
                                                            toggle_recording(Arc::clone(&state_wd));
                                                            break;
                                                        }
                                                    }
                                                });
                                            }
                                            Err(e) => println!("❌ Fn kayıt hatası: {}", e),
                                        }
                                    });
                                } else {
                                    let state_stop = Arc::clone(&state_cb);
                                    std::thread::spawn(move || {
                                        toggle_recording(state_stop);
                                    });
                                }
                            } else {
                                *last = now;
                            }
                        }
                    }
                });
                let block = block.copy();
                
                unsafe {
                    let cls = objc::runtime::Class::get("NSEvent").unwrap();
                    let _: id = msg_send![cls, addGlobalMonitorForEventsMatchingMask:mask handler:&*block];
                }
                // block'u leak et ki yaşamaya devam etsin
                std::mem::forget(block);
                println!("🎹 NSEvent global monitor aktif — Fn double-tap dinleniyor");
            }

            println!("🌿 Millow başlatıldı!");
            println!("   Kısayollar: {} veya Fn tuşuna çift tıkla", hotkey_str);
            println!("   Tray menüsünden de kullanabilirsiniz");

            // Ana pencereyi gizle ve Dock'tan kaldır (menü çubuğu uygulaması)
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide().unwrap();
            }
            app_state.window_visible.store(false, std::sync::atomic::Ordering::Relaxed);
            #[cfg(target_os = "macos")]
            hide_dock();

            // Pencere kapatma olayını yakala — gizle, çıkma
            let app_handle = app.handle().clone();
            let state_for_close = app_state.clone();
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Kapatmayı engelle, sadece gizle
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                        state_for_close.window_visible.store(false, std::sync::atomic::Ordering::Relaxed);
                        #[cfg(target_os = "macos")]
                        hide_dock();
                        println!("🪟 Pencere gizlendi, arka planda çalışıyor");
                    }
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Millow başlatılırken hata oluştu");

    // Son pencere kapansa bile uygulamayı arka planda çalıştır
    app.run(|_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            // Çıkışı engelle — menü bardan "Çıkış" tıklanmadıkça kapanma
            api.prevent_exit();
        }
    });
}
