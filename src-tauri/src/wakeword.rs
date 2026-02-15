// Millow — Wake Word Dinleyici
// Sürekli mikrofonu dinler, ses algılandığında kısa buffer'ı Gemini'ye gönderir
// "millow" algılanırsa kayda geçer

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// Stream'i thread-safe tutmak için wrapper
struct StreamHolder(Stream);
unsafe impl Send for StreamHolder {}
unsafe impl Sync for StreamHolder {}

/// Wake word dinleyici
pub struct WakeWordListener {
    is_listening: Arc<AtomicBool>,
    buffer: Arc<Mutex<Vec<i16>>>,
    voice_detected: Arc<AtomicBool>,
    sample_rate: Arc<Mutex<u32>>,
    _stream: Mutex<Option<StreamHolder>>,
}

impl WakeWordListener {
    pub fn new() -> Self {
        Self {
            is_listening: Arc::new(AtomicBool::new(false)),
            buffer: Arc::new(Mutex::new(Vec::new())),
            voice_detected: Arc::new(AtomicBool::new(false)),
            sample_rate: Arc::new(Mutex::new(48000)),
            _stream: Mutex::new(None),
        }
    }

    /// Sürekli dinlemeyi başlat
    pub fn start_listening<F>(&self, on_wake: F) -> Result<(), String>
    where
        F: Fn() + Send + Sync + 'static,
    {
        if self.is_listening.load(Ordering::SeqCst) {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("Mikrofon bulunamadı")?;

        let default_config = device
            .default_input_config()
            .map_err(|e| format!("Mikrofon config hatası: {}", e))?;

        let device_sr = default_config.sample_rate().0;
        let channels = default_config.channels() as usize;
        *self.sample_rate.lock() = device_sr;

        let max_samples = (device_sr as usize) * 3;

        let config = cpal::StreamConfig {
            channels: default_config.channels(),
            sample_rate: cpal::SampleRate(device_sr),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = self.buffer.clone();
        let voice_detected = self.voice_detected.clone();
        let is_listening = self.is_listening.clone();
        let energy_threshold: f32 = 0.01;
        let silence_counter = Arc::new(Mutex::new(0u32));
        let voice_counter = Arc::new(Mutex::new(0u32));
        let silence_c = silence_counter.clone();
        let voice_c = voice_counter.clone();

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !is_listening.load(Ordering::SeqCst) {
                        return;
                    }

                    let mono: Vec<i16> = if channels > 1 {
                        data.chunks(channels)
                            .map(|frame| (frame[0] * 32767.0).clamp(-32768.0, 32767.0) as i16)
                            .collect()
                    } else {
                        data.iter()
                            .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
                            .collect()
                    };

                    let energy: f32 = data.iter().map(|&s| s * s).sum::<f32>() / data.len() as f32;

                    let mut buf = buffer.lock();
                    buf.extend_from_slice(&mono);
                    if buf.len() > max_samples {
                        let excess = buf.len() - max_samples;
                        buf.drain(..excess);
                    }

                    if energy > energy_threshold {
                        *silence_c.lock() = 0;
                        let mut vc = voice_c.lock();
                        *vc += 1;
                        if *vc > 15 && !voice_detected.load(Ordering::SeqCst) {
                            voice_detected.store(true, Ordering::SeqCst);
                        }
                    } else {
                        let mut sc = silence_c.lock();
                        *sc += 1;
                        if voice_detected.load(Ordering::SeqCst) && *sc > 40 {
                            voice_detected.store(false, Ordering::SeqCst);
                            *voice_c.lock() = 0;
                            *sc = 0;
                        }
                    }
                },
                |err| eprintln!("Wake word stream hatası: {}", err),
                None,
            )
            .map_err(|e| format!("Wake word stream oluşturulamadı: {}", e))?;

        stream.play().map_err(|e| format!("Wake word stream başlatılamadı: {}", e))?;

        // Stream'i sakla
        *self._stream.lock() = Some(StreamHolder(stream));
        self.is_listening.store(true, Ordering::SeqCst);

        // Wake word kontrol thread'i
        let buffer_check = self.buffer.clone();
        let voice_det = self.voice_detected.clone();
        let sr = self.sample_rate.clone();
        let listening = self.is_listening.clone();
        let on_wake = Arc::new(on_wake);

        std::thread::spawn(move || {
            println!("👂 Wake word dinleyicisi aktif");

            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));

                if !listening.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    continue;
                }

                if voice_det.load(Ordering::SeqCst) {
                    let samples = buffer_check.lock().clone();
                    let rate = *sr.lock();

                    if samples.len() > (rate as usize) {
                        voice_det.store(false, Ordering::SeqCst);

                        match check_wake_word(&samples, rate) {
                            Ok(true) => {
                                println!("🌿 Wake word algılandı: MILLOW!");
                                buffer_check.lock().clear();
                                on_wake();
                            }
                            Ok(false) => {}
                            Err(e) => {
                                println!("⚠️  Wake word kontrol hatası: {}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    pub fn pause(&self) {
        self.is_listening.store(false, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.buffer.lock().clear();
        self.voice_detected.store(false, Ordering::SeqCst);
        self.is_listening.store(true, Ordering::SeqCst);
    }
}

fn check_wake_word(samples: &[i16], source_rate: u32) -> Result<bool, String> {
    use crate::audio::AudioEngine;
    use crate::config::MillowConfig;
    use base64::Engine as _;

    let config = MillowConfig::load();
    let wav_bytes = AudioEngine::samples_to_wav(samples, source_rate)?;
    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);

    let url = format!(
        "{}/v1beta/models/{}:generateContent?key={}",
        config.proxy_endpoint, config.model, config.api_key
    );

    let body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                {"text": "Bu kısa ses kaydında 'millow' veya 'milo' kelimesi var mı? SADECE 'evet' veya 'hayir' yaz."},
                {"inline_data": {"mime_type": "audio/wav", "data": audio_b64}}
            ]
        }]
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Client hatası: {}", e))?;

    let response = client.post(&url).json(&body).send()
        .map_err(|e| format!("Wake word API hatası: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("API hatası: {}", response.status()));
    }

    let json: serde_json::Value = response.json()
        .map_err(|e| format!("JSON parse hatası: {}", e))?;

    let text = json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();

    Ok(text.contains("evet"))
}
