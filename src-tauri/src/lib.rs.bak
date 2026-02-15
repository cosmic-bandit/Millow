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
use std::sync::Arc;
use tauri::{
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Manager, WebviewWindow,
};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use rdev::{listen, Event, EventType, Key};

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
                notify("🎙️ Kayıt", "Konuşun, bitince tekrar basın");
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
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            &format!(
                "display notification \"{}\" with title \"{}\"",
                message.replace('"', "'"),
                title.replace('"', "'")
            ),
        ])
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
    let _ = app.global_shortcut().unregister(old_hotkey.as_str());
    
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
        ])
        .setup(move |app| {
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

            // ── Double-Tap Fn Tuşu Dinleyicisi (rdev) + 30s Sessizlik Watchdog ──
            let state_for_fn = state_for_manager.clone();
            std::thread::spawn(move || {
                println!("🎹 Double-tap Fn dinleyicisi başlatıldı");
                let state = state_for_fn;
                let last_fn_press = Arc::new(Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(10)));
                
                if let Err(error) = listen(move |event: Event| {
                    match event.event_type {
                        EventType::KeyPress(Key::Function) => {
                            let now = std::time::Instant::now();
                            let mut last = last_fn_press.lock();
                            let elapsed = now.duration_since(*last);
                            
                            if elapsed.as_millis() < 400 {
                                // Double-tap algılandı!
                                println!("🎹 Double-tap Fn algılandı! ({:.0}ms)", elapsed.as_millis());
                                *last = now - std::time::Duration::from_secs(10); // Reset
                                
                                let is_rec = *state.is_recording.lock();
                                if !is_rec {
                                    // Kayıt başlat + sessizlik watchdog'u kur
                                    let state_start = Arc::clone(&state);
                                    std::thread::spawn(move || {
                                        match state_start.audio_engine.lock().start_recording() {
                                            Ok(_) => {
                                                *state_start.source_app.lock() = get_active_app();
                                                *state_start.is_recording.lock() = true;
                                                println!("🎙️  Fn kayıt başladı (hedef: {:?})", state_start.source_app.lock());
                                                notify("🎙️ Kayıt", "Konuşun, 30s sessizlikte otomatik durur");
                                                
                                                // Sessizlik watchdog: her 2s kontrol, 30s sessizlikte durdur
                                                let state_wd = Arc::clone(&state_start);
                                                std::thread::spawn(move || {
                                                    loop {
                                                        std::thread::sleep(std::time::Duration::from_secs(2));
                                                        let is_rec = *state_wd.is_recording.lock();
                                                        if !is_rec {
                                                            break; // Kullanıcı zaten durdurdu
                                                        }
                                                        let silence_secs = state_wd.audio_engine.lock().seconds_since_voice();
                                                        if silence_secs >= 30.0 {
                                                            println!("🔇 30s sessizlik — otomatik durdurma");
                                                            notify("🔇 Sessizlik", "30s ses gelmedi, durduruldu");
                                                            toggle_recording(Arc::clone(&state_wd));
                                                            break;
                                                        }
                                                    }
                                                });
                                            }
                                            Err(e) => {
                                                println!("❌ Fn kayıt hatası: {}", e);
                                            }
                                        }
                                    });
                                } else {
                                    // Kayıt zaten varsa durdur
                                    let state_stop = Arc::clone(&state);
                                    std::thread::spawn(move || {
                                        toggle_recording(state_stop);
                                    });
                                }
                            } else {
                                // İlk basış — zamanı kaydet
                                *last = now;
                            }
                        }
                        _ => {}
                    }
                }) {
                    println!("❌ rdev dinleme hatası: {:?}", error);
                }
            });
            println!("🌿 Millow başlatıldı!");
            println!("   Kısayollar: {} veya Fn tuşuna çift tıkla", hotkey_str);
            println!("   Tray menüsünden de kullanabilirsiniz");

            // Ana pencereyi gizle ve Dock'tan kaldır (menü çubuğu uygulaması)
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide().unwrap();
            }
            #[cfg(target_os = "macos")]
            hide_dock();

            // Pencere kapatma olayını yakala — gizle, çıkma
            let app_handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Kapatmayı engelle, sadece gizle
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
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
