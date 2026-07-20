// Millow — Ses Kayıt Motoru
// cpal ile mikrofon kaydı, WAV formatına çevirme

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use rubato::Resampler;

/// Ses kayıt motoru durumu
#[derive(Debug, Clone, PartialEq)]
pub enum RecordingState {
    Idle,
    Recording,
}

// Stream'i thread-safe tutmak için wrapper
struct StreamHolder(Stream);
unsafe impl Send for StreamHolder {}
unsafe impl Sync for StreamHolder {}

struct SendVad(webrtc_vad::Vad);
unsafe impl Send for SendVad {}
unsafe impl Sync for SendVad {}

/// Ses kayıt motoru
pub struct AudioEngine {
    state: Arc<Mutex<RecordingState>>,
    samples: Arc<Mutex<Vec<i16>>>,
    actual_sample_rate: Arc<Mutex<u32>>,
    /// Aktif stream — stop'ta drop edilir
    active_stream: Mutex<Option<StreamHolder>>,
    /// Son ses aktivitesi zamanı (sessizlik algılama için)
    last_voice_activity: Arc<Mutex<std::time::Instant>>,
    /// WebRTC VAD (ve desteklenmeyen hızlardaki fallback) tarafından algılanan
    /// toplam ses aktivitesi. Watchdog gerçek konuşmayı kayıt başlangıcındaki
    /// yapay zaman damgasından ayırmak için bu monoton sayacı kullanır.
    voice_activity_count: Arc<AtomicU64>,
    /// Ortam gürültüsü toleransı
    noise_tolerance: Arc<Mutex<f32>>,
}

impl AudioEngine {
    pub fn new(_sample_rate: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            samples: Arc::new(Mutex::new(Vec::new())),
            actual_sample_rate: Arc::new(Mutex::new(16000)),
            active_stream: Mutex::new(None),
            last_voice_activity: Arc::new(Mutex::new(std::time::Instant::now())),
            voice_activity_count: Arc::new(AtomicU64::new(0)),
            noise_tolerance: Arc::new(Mutex::new(0.15)),
        }
    }

    pub fn get_actual_sample_rate(&self) -> u32 {
        *self.actual_sample_rate.lock()
    }

    /// Kaydı başlat
    pub fn start_recording(&self) -> Result<(), String> {
        // Önceki stream varsa temizle
        {
            let mut stream_guard = self.active_stream.lock();
            *stream_guard = None;
        }

        let mut state = self.state.lock();
        if *state == RecordingState::Recording {
            return Err("Zaten kayıt yapılıyor".into());
        }

        self.samples.lock().clear();
        *self.noise_tolerance.lock() = crate::config::MillowConfig::load().noise_tolerance;
        *self.last_voice_activity.lock() = std::time::Instant::now();
        self.voice_activity_count.store(0, Ordering::Relaxed);
        *state = RecordingState::Recording;
        drop(state); // Lock'u serbest bırak

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("Mikrofon bulunamadı")?;

        let default_config = device
            .default_input_config()
            .map_err(|e| format!("Mikrofon yapılandırması alınamadı: {}", e))?;

        let device_sample_rate = default_config.sample_rate().0;
        let device_channels = default_config.channels();
        let sample_format = default_config.sample_format();

        println!(
            "🎙️  Mikrofon: {}Hz, {} kanal, {:?}",
            device_sample_rate, device_channels, sample_format
        );

        *self.actual_sample_rate.lock() = device_sample_rate;

        let config = cpal::StreamConfig {
            channels: device_channels,
            sample_rate: cpal::SampleRate(device_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let samples = self.samples.clone();
        let state_clone = self.state.clone();
        let channels = device_channels as usize;
        let voice_ts = self.last_voice_activity.clone();
        let voice_count = self.voice_activity_count.clone();
        let noise_tol = *self.noise_tolerance.lock();
        let silence_threshold: i16 = (noise_tol * 32767.0) as i16; // ~1.5% of max

        let stream = match sample_format {
            cpal::SampleFormat::I16 => {
                let mut vad: Option<SendVad> = match device_sample_rate {
                    8000 => Some(SendVad(webrtc_vad::Vad::new_with_rate_and_mode(
                        webrtc_vad::SampleRate::Rate8kHz,
                        webrtc_vad::VadMode::Aggressive,
                    ))),
                    16000 => Some(SendVad(webrtc_vad::Vad::new_with_rate_and_mode(
                        webrtc_vad::SampleRate::Rate16kHz,
                        webrtc_vad::VadMode::Aggressive,
                    ))),
                    32000 => Some(SendVad(webrtc_vad::Vad::new_with_rate_and_mode(
                        webrtc_vad::SampleRate::Rate32kHz,
                        webrtc_vad::VadMode::Aggressive,
                    ))),
                    48000 => Some(SendVad(webrtc_vad::Vad::new_with_rate_and_mode(
                        webrtc_vad::SampleRate::Rate48kHz,
                        webrtc_vad::VadMode::Aggressive,
                    ))),
                    _ => None,
                };

                let vad_frame_size = (device_sample_rate / 100) as usize;
                let mut vad_buffer = Vec::new();

                device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let current_state = state_clone.lock();
                        if *current_state == RecordingState::Recording {
                            let mono: Vec<i16> = if channels > 1 {
                                data.chunks(channels)
                                    .map(|frame| frame[0])
                                    .collect()
                            } else {
                                data.to_vec()
                            };

                            if let Some(ref mut v) = vad {
                                vad_buffer.extend_from_slice(&mono);
                                while vad_buffer.len() >= vad_frame_size {
                                    let frame: Vec<i16> = vad_buffer.drain(0..vad_frame_size).collect();
                                    match v.0.is_voice_segment(&frame) {
                                        Ok(true) => {
                                            *voice_ts.lock() = std::time::Instant::now();
                                            voice_count.fetch_add(1, Ordering::Relaxed);
                                        }
                                        _ => {}
                                    }
                                }
                            } else {
                                if mono.iter().any(|&s| s.abs() > silence_threshold) {
                                    *voice_ts.lock() = std::time::Instant::now();
                                    voice_count.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            samples.lock().extend_from_slice(&mono);
                        }
                    },
                    |err| eprintln!("Ses akışı hatası: {}", err),
                    None,
                )
            }
            cpal::SampleFormat::F32 => {
                let samples2 = self.samples.clone();
                let state_clone2 = self.state.clone();
                let voice_ts2 = self.last_voice_activity.clone();
                let voice_count2 = self.voice_activity_count.clone();
                let silence_threshold_f: f32 = *self.noise_tolerance.lock();

                let mut vad: Option<SendVad> = match device_sample_rate {
                    8000 => Some(SendVad(webrtc_vad::Vad::new_with_rate_and_mode(
                        webrtc_vad::SampleRate::Rate8kHz,
                        webrtc_vad::VadMode::Aggressive,
                    ))),
                    16000 => Some(SendVad(webrtc_vad::Vad::new_with_rate_and_mode(
                        webrtc_vad::SampleRate::Rate16kHz,
                        webrtc_vad::VadMode::Aggressive,
                    ))),
                    32000 => Some(SendVad(webrtc_vad::Vad::new_with_rate_and_mode(
                        webrtc_vad::SampleRate::Rate32kHz,
                        webrtc_vad::VadMode::Aggressive,
                    ))),
                    48000 => Some(SendVad(webrtc_vad::Vad::new_with_rate_and_mode(
                        webrtc_vad::SampleRate::Rate48kHz,
                        webrtc_vad::VadMode::Aggressive,
                    ))),
                    _ => None,
                };

                let vad_frame_size = (device_sample_rate / 100) as usize;
                let mut vad_buffer = Vec::new();

                device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let current_state = state_clone2.lock();
                        if *current_state == RecordingState::Recording {
                            let mono: Vec<i16> = if channels > 1 {
                                data.chunks(channels)
                                    .map(|frame| (frame[0] * 32767.0).clamp(-32768.0, 32767.0) as i16)
                                    .collect()
                            } else {
                                data.iter()
                                    .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
                                    .collect()
                            };

                            if let Some(ref mut v) = vad {
                                vad_buffer.extend_from_slice(&mono);
                                while vad_buffer.len() >= vad_frame_size {
                                    let frame: Vec<i16> = vad_buffer.drain(0..vad_frame_size).collect();
                                    match v.0.is_voice_segment(&frame) {
                                        Ok(true) => {
                                            *voice_ts2.lock() = std::time::Instant::now();
                                            voice_count2.fetch_add(1, Ordering::Relaxed);
                                        }
                                        _ => {}
                                    }
                                }
                            } else {
                                if data.iter().any(|&s| s.abs() > silence_threshold_f) {
                                    *voice_ts2.lock() = std::time::Instant::now();
                                    voice_count2.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                            samples2.lock().extend_from_slice(&mono);
                        }
                    },
                    |err| eprintln!("Ses akışı hatası: {}", err),
                    None,
                )
            }
            _ => return Err(format!("Desteklenmeyen ses formatı: {:?}", sample_format)),
        }
        .map_err(|e| format!("Ses akışı oluşturulamadı: {}", e))?;

        stream.play().map_err(|e| format!("Akış başlatılamadı: {}", e))?;

        // Stream'i sakla (drop edilene kadar kayıt devam eder)
        *self.active_stream.lock() = Some(StreamHolder(stream));
        println!("✅ Audio stream başlatıldı");

        Ok(())
    }

    /// Kaydı durdur ve örnekleri döndür
    pub fn stop_recording(&self) -> Vec<i16> {
        // State'i Idle yap
        *self.state.lock() = RecordingState::Idle;

        // Stream'i drop et — mikrofonu serbest bırakır
        {
            let mut stream_guard = self.active_stream.lock();
            *stream_guard = None;
            println!("🛑 Audio stream durduruldu");
        }

        self.samples.lock().clone()
    }

    /// Buffer'daki sesleri al ve temizle, ama kayda devam et (segment flush)
    pub fn set_noise_tolerance(&self, val: f32) {
        *self.noise_tolerance.lock() = val;
    }

    pub fn drain_samples(&self) -> Vec<i16> {
        let mut samples = self.samples.lock();
        let drained = samples.clone();
        samples.clear();
        drained
    }

    pub fn samples_len(&self) -> usize {
        self.samples.lock().len()
    }

    pub fn is_recording(&self) -> bool {
        *self.state.lock() == RecordingState::Recording
    }

    /// Son ses aktivitesinden bu yana geçen süre (saniye)
    pub fn seconds_since_voice(&self) -> f64 {
        self.last_voice_activity.lock().elapsed().as_secs_f64()
    }

    /// Mevcut kayıt boyunca algılanan gerçek ses aktivitesi sayacı.
    pub fn voice_activity_count(&self) -> u64 {
        self.voice_activity_count.load(Ordering::Relaxed)
    }

    /// PCM örneklerini WAV bytes'a çevir (16kHz mono çıktı)
    pub fn samples_to_wav(samples: &[i16], source_rate: u32) -> Result<Vec<u8>, String> {
        let target_rate: u32 = 16000;

        let final_samples = if source_rate != target_rate && source_rate > 0 {
            let f32_samples: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();

            let params = rubato::SincInterpolationParameters {
                sinc_len: 64,
                f_cutoff: 0.95,
                oversampling_factor: 32,
                interpolation: rubato::SincInterpolationType::Linear,
                window: rubato::WindowFunction::BlackmanHarris2,
            };

            let ratio = target_rate as f64 / source_rate as f64;
            let chunk_size = 1024;
            let mut resampler = rubato::SincFixedIn::<f32>::new(
                ratio,
                1.0,
                params,
                chunk_size,
                1, // 1 channel
            ).map_err(|e| format!("Resampler başlatılamadı: {:?}", e))?;

            let output_delay = resampler.output_delay();
            let input_delay = (output_delay as f64 / ratio) as usize;

            // Gecikmeyi kurtarmak için sonuna sessizlik ekliyoruz
            let mut padded_samples = f32_samples;
            padded_samples.resize(padded_samples.len() + input_delay, 0.0);

            // chunk_size katı olacak şekilde sıfırlarla tamamlıyoruz
            let rem = padded_samples.len() % chunk_size;
            if rem > 0 {
                padded_samples.resize(padded_samples.len() + (chunk_size - rem), 0.0);
            }

            let mut output_samples = Vec::new();
            let mut input_buffer = vec![vec![0.0f32; chunk_size]; 1];

            let mut pos = 0;
            while pos < padded_samples.len() {
                input_buffer[0].copy_from_slice(&padded_samples[pos..pos + chunk_size]);
                let processed = resampler.process(&input_buffer, None)
                    .map_err(|e| format!("Resampling hatası: {:?}", e))?;
                output_samples.extend_from_slice(&processed[0]);
                pos += chunk_size;
            }

            let target_len = (samples.len() as f64 * ratio) as usize;
            let mut final_resampled = Vec::with_capacity(target_len);

            // Gecikmeyi telafi edip target_len kadarını alıyoruz
            let start_idx = output_delay;
            let end_idx = start_idx + target_len;

            if output_samples.len() >= end_idx {
                for &s in &output_samples[start_idx..end_idx] {
                    let s_val = s as f32;
                    final_resampled.push((s_val * 32767.0).clamp(-32768.0, 32767.0) as i16);
                }
            } else {
                for &s in output_samples.iter().skip(start_idx) {
                    let s_val = s as f32;
                    final_resampled.push((s_val * 32767.0).clamp(-32768.0, 32767.0) as i16);
                }
            }

            println!(
                "🔄 Downsample (rubato): {}Hz → {}Hz ({} → {} samples, delay compensated)",
                source_rate, target_rate, samples.len(), final_resampled.len()
            );
            final_resampled
        } else {
            samples.to_vec()
        };

        let mut cursor = std::io::Cursor::new(Vec::new());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: target_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| format!("WAV yazıcı oluşturulamadı: {}", e))?;

        for &sample in &final_samples {
            writer.write_sample(sample)
                .map_err(|e| format!("Örnek yazılamadı: {}", e))?;
        }

        writer.finalize()
            .map_err(|e| format!("WAV sonlandırılamadı: {}", e))?;

        Ok(cursor.into_inner())
    }
}
