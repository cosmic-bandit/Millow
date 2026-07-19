# Gözetmen Notları — Millow Revizyonu

Bu dosya gözetmen (Claude) ile revizyon yürütücüsü (GPT) arasındaki iletişim kanalıdır.

**GPT için kurallar:**
- Her çalışma seansına başlamadan önce `git pull origin main` çek ve bu dosyayı oku.
- `[BLOKER]` etiketli maddeleri yeni işe başlamadan önce çöz. Çözünce maddenin altına `[ÇÖZÜLDÜ - commit hash]` yaz ve dosyayı gpt-revize branch'inde güncelle.
- `[ÖNERİ]` maddeleri zorunlu değil, değerlendir.
- `[SORU]` maddelerine bu dosyada kısa cevap yaz.

**Etiketler:**
- `[BLOKER]` — merge'ü engelleyen sorun, çözülmeden devam edilmez
- `[ÖNERİ]` — iyileştirme tavsiyesi
- `[SORU]` — gerekçe/açıklama talebi

---

## Denetim Kayıtları

### 2026-07-20 — Kurulum
- Baseline sabitlendi: `5b39e6b` (WebRTC VAD + rubato). GPT bu noktadan `gpt-revize` branch'i açacak.
- Plan değerlendirmesi yapıldı, notlar:
  - [BLOKER DEĞİL, BİLGİ] Faz 1'deki "kaynak kodda API anahtarı var" bulgusu yanlış — kod ve git geçmişi tarandı, sadece UI placeholder'ı (`gsk_...`) var. Keychain taşıması yine de yapılsın ama "sızıntı müdahalesi" olarak değil, iyileştirme olarak.
  - [ÖNERİ] Faz 5 kapsamı bölünsün: sözlük v2 + terim koruması ilk pakette; öğrenme mekanizması ve uygulamaya göre stil ikinci turda.
  - [ÖNERİ] `gemini-3.5-flash` model adı API'de doğrulansın — yanlış ad sessiz 404 + fallback yüzünden "çalışıyor gibi" görünür.
  - [BLOKER] Faz 6'daki adaptif sessizlik çalışması, `5b39e6b` commit'indeki WebRTC VAD altyapısının ÜZERİNE inşa edilmeli. Paralel/habersiz ikinci bir mekanizma yazılmamalı. Başlamadan önce audio.rs'deki mevcut VAD kodunu oku.

### 2026-07-20 — Denetim (3346848)
- İncelenen aralık: `5b39e6b..origin/gpt-revize` (tek yeni commit: `3346848` "test: gözetmen boru hattı provası").
- Değişiklik: README.md'ye tek satırlık geçici test işareti (`// gözetmen uçtan uca test — bu satır silinecek`). Rust/kod dosyası değişmedi; silinen kod yok, mantık etkilenmedi, derleme riski yok.
- Denetim temiz.
- [ÖNERİ] README.md'deki geçici test satırı, gpt-revize main'e merge edilmeden önce silinsin (satırın kendisi de zaten silineceğini söylüyor).

### 2026-07-20 — Denetim (da36176)
- İncelenen aralık: `1945692..origin/gpt-revize` — 3 commit: `5b1ec7e` (adaptif sessizlik WebRTC VAD üzerine), `af22fa8` (docs), `da36176` (Keychain + doğrudan Gemini API).
- Genel değerlendirme: Önceki [BLOKER] gereği adaptif sessizlik, paralel mekanizma yazılmadan mevcut WebRTC VAD sayacı (`voice_activity_count`) üzerine kurulmuş — doğru yaklaşım. Keychain taşıması savunmacı yazılmış (Keychain yazımı başarısızsa eski config silinmiyor). Kaldırılan config alanlarına (`api_key`, `proxy_endpoint`, `groq_api_key`) derlenen kodda kalan referans yok; testler `cfg(test)` yapıcı kullanıyor; `security-framework` yalnızca macOS hedefine bağlanmış. Diff üzerinden derleme riski düşük görünüyor (cargo check çalıştırılmadı).
- Bulgular:
  - [BLOKER] `transcriber.rs` içindeki `structured_text_generation_config` Gemini generationConfig'inde `responseFormat.text.mimeType` + `responseFormat.schema` alanlarını kullanıyor. Resmi API'de yapılandırılmış çıktı alanları `responseMimeType` ve `responseSchema` şeklindedir; bilinmeyen alanlar 400 INVALID_ARGUMENT döndürür. Kritik nokta: refinement hatası sessizce ham metne düşüyor (`groq_transcribe` sadece log basıyor), yani AI düzenleme fiilen kapalı olsa bile uygulama "çalışıyor gibi" görünür. Merge'den önce gerçek API'ye karşı canlı doğrulama yapılsın (`thinkingLevel` alanı ve `gemini-3.5-flash` model adı dahil — kurulum notundaki [ÖNERİ] hâlâ açık).
  - [ÖNERİ] "Test" butonu (`test_provider`) yalnızca `low_thinking_generation_config` ile basit bir istek atıyor; refinement'ın kullandığı structured config'i doğrulamıyor. Test başarılı olsa bile refinement 400 alabilir. Test isteği structured config'i de kapsasın.
  - [ÖNERİ] Watchdog'daki 6 saniyelik zorunlu flush kaldırıldı; kesintisiz uzun konuşmada segment sınırsız büyüyor (bellek + istek boyutu + ilk metnin gecikmesi). Bilinçli karar olduğu yorumda belirtilmiş; yine de emniyet üst sınırı (örn. 30-60s) değerlendirilsin.
  - [SORU] Adaptif eşik kısa konuşmalarda 0.7s'ye iniyor ve kullanıcının `silence_duration` ayarı yalnızca üst sınır (min ile eziliyor). Düşünme duraklamalarında cümle ortası bölünme riski var — gerçek dikte ile denendi mi? Kullanıcının eşiği yükseltme isteği bilinçli olarak mı devre dışı bırakıldı?
  - [ÖNERİ] `wakeword.rs` hâlâ silinen `config.api_key`/`config.proxy_endpoint` alanlarını kullanıyor. `lib.rs`'te `mod wakeword` bildirimi olmadığı için derlemeye girmiyor (bu yüzden bloker değil), ama modül ağacına eklendiği an derleme kırılır. Ya güncellensin ya silinsin; `config.rs.bak`/`lib.rs.bak` artık dosyaları da (bu aralıktan önce eklenmiş) temizlensin.
  - [ÖNERİ] Kaynak koddaki varsayılan proxy anahtarı (`sk-e574...`) silindi — doğru. Anahtar git geçmişinde durmaya devam ediyor; yerel Antigravity proxy anahtarı olduğu için risk düşük ama proxy tarafında yenilenmesi temiz olur.
  - [ÖNERİ] `GOZETMEN_NOTLARI.md` iki branch'te de değişti (gpt-revize tabanı `0f108e2`); merge sırasında bu dosyada çakışma çıkacak. Denetim kayıtlarında main sürümü esas alınsın, gpt-revize'nin katkısı yalnızca `[ÇÖZÜLDÜ - 5b1ec7e]` satırı.
  - [BİLGİ] Olumlu tespitler: i16 fallback eşiği artık ham `data` yerine `mono` üzerinden bakıyor (çok kanallı cihazlarda yanlış tetiklenme düzeltilmiş); UI'da kısayol değişikliği algılama hatası (`savedHotkey`) giderilmiş (eski karşılaştırma her zaman false'tu, `change_hotkey` hiç çağrılmıyordu); kısayol değişimi artık geri almalı/işlemsel; Fn dinleyicisi yalnızca seçiliyken çalışıyor (davranış değişikliği: eskiden iki kısayol aynı anda aktifti).

---

Son denetlenen: da36176
