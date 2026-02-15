// Millow — Ses Kayıt Motoru
// cpal ile mikrofon kaydı, WAV formatına çevirme

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use parking_lot::Mutex;
use std::sync::Arc;

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

/// Ses kayıt motoru
pub struct AudioEngine {
    state: Arc<Mutex<RecordingState>>,
    samples: Arc<Mutex<Vec<i16>>>,
    actual_sample_rate: Arc<Mutex<u32>>,
    /// Aktif stream — stop'ta drop edilir
    active_stream: Mutex<Option<StreamHolder>>,
    /// Son ses aktivitesi zamanı (sessizlik algılama için)
    last_voice_activity: Arc<Mutex<std::time::Instant>>,
}

impl AudioEngine {
    pub fn new(_sample_rate: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            samples: Arc::new(Mutex::new(Vec::new())),
            actual_sample_rate: Arc::new(Mutex::new(16000)),
            active_stream: Mutex::new(None),
            last_voice_activity: Arc::new(Mutex::new(std::time::Instant::now())),
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
        *self.last_voice_activity.lock() = std::time::Instant::now();
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
        let silence_threshold: i16 = 500; // ~1.5% of max

        let stream = match sample_format {
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let current_state = state_clone.lock();
                        if *current_state == RecordingState::Recording {
                            // Sessizlik algılama: herhangi bir sample eşiği aşıyorsa aktivite var
                            if data.iter().any(|&s| s.abs() > silence_threshold) {
                                *voice_ts.lock() = std::time::Instant::now();
                            }
                            if channels > 1 {
                                let mono: Vec<i16> = data
                                    .chunks(channels)
                                    .map(|frame| frame[0])
                                    .collect();
                                samples.lock().extend_from_slice(&mono);
                            } else {
                                samples.lock().extend_from_slice(data);
                            }
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
                let silence_threshold_f: f32 = 0.015;
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let current_state = state_clone2.lock();
                        if *current_state == RecordingState::Recording {
                            if data.iter().any(|&s| s.abs() > silence_threshold_f) {
                                *voice_ts2.lock() = std::time::Instant::now();
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

    pub fn is_recording(&self) -> bool {
        *self.state.lock() == RecordingState::Recording
    }

    /// Son ses aktivitesinden bu yana geçen süre (saniye)
    pub fn seconds_since_voice(&self) -> f64 {
        self.last_voice_activity.lock().elapsed().as_secs_f64()
    }

    /// PCM örneklerini WAV bytes'a çevir (16kHz mono çıktı)
    pub fn samples_to_wav(samples: &[i16], source_rate: u32) -> Result<Vec<u8>, String> {
        let target_rate: u32 = 16000;

        let final_samples = if source_rate != target_rate && source_rate > 0 {
            let ratio = source_rate as f64 / target_rate as f64;
            let new_len = (samples.len() as f64 / ratio) as usize;
            let mut resampled = Vec::with_capacity(new_len);

            for i in 0..new_len {
                let src_pos = i as f64 * ratio;
                let idx = src_pos as usize;
                let frac = src_pos - idx as f64;

                if idx + 1 < samples.len() {
                    let s = samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac;
                    resampled.push(s.clamp(-32768.0, 32767.0) as i16);
                } else if idx < samples.len() {
                    resampled.push(samples[idx]);
                }
            }
            println!("🔄 Downsample: {}Hz → {}Hz ({} → {} samples)", source_rate, target_rate, samples.len(), resampled.len());
            resampled
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
