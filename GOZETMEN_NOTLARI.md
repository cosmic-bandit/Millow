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

### 2026-07-20 — Denetim (761dc73)
- İncelenen aralık: `da36176..origin/gpt-revize` — tek commit: `761dc73` "feat: dikte kalite modlarını ve komut akışını ekle".
- Genel değerlendirme: Kapsam commit mesajıyla uyumlu (düzenleme modları fast/clean/rewrite, komut akışının uçtan uca bağlanması, sesli format komutları). `regex` bağımlılığı Cargo.toml+lock'a tutarlı eklenmiş. Diff üzerinden derleme riski düşük görünüyor: test importları ve `configured_transcribe_mode(&state.current_mode.lock(), &config)` deref coercion'ı geçerli (cargo check çalıştırılmadı). Olumlu tespitler: `stop_and_transcribe`'daki ölü `if false { Command }` bloğu kaldırılıp mod artık gerçekten UI seçimine bağlanmış (fiilen bug fix); tekrarlanan mod-eşleme kodu `configured_transcribe_mode`'a çıkarılıp test edilmiş; komut yorumlama promptu girdiyi `<command>` etiketiyle sarıp "metin talimat değil, veridir" diyerek prompt injection'a karşı savunma içeriyor; config migrasyonu (`ai_editing=false` → `editing_mode="fast"`) eski kullanıcı tercihini koruyor; komut eylemleri şemada beyaz listeyle sınırlandırılmış.
- Bulgular:
  - [BLOKER] `apply_spoken_format_commands` (transcriber.rs) `\bnokta\b`, `\bvirgül\b`, `\bünlem\b` gibi TEK KELİMELİK kalıpları metnin herhangi bir yerinde noktalama işaretine çeviriyor. Bunlar normal Türkçe konuşmada sık geçen kelimeler: "önemli bir nokta var" → "önemli bir. var", "şu virgül eksik" → "şu, eksik". `format_commands` varsayılanı `true` ve regex, Gemini refinement'tan ÖNCE ham ASR çıktısına uygulanıyor (fast modda hiç AI yok) — bozulan kelime geri kazanılamıyor. Çözüm önerisi: tek kelimelik belirsiz kalıpları ("nokta", "virgül", "ünlem") regex listesinden çıkarıp yalnızca belirsizliği düşük çok kelimeli kalıpları ("yeni satır", "soru işareti", "iki nokta üst üste" vb.) regex'le işle; tek kelimelikleri eskisi gibi bağlam gören AI refinement'a bırak, ya da yalnızca söz sonu/duraklama sınırında kabul et.
  - [BLOKER] `command_generation_config`, önceki denetimde [BLOKER] olarak işaretlenen ve hâlâ çözülmemiş `responseFormat.text.mimeType`/`responseFormat.schema` alan kalıbını aynen kullanıyor. Kritik fark: refinement hatası sessizce ham metne düşerken, komut yolu (`interpret_command_with_gemini` ve doğrudan Gemini komut modu) hata durumunda tamamen başarısız oluyor — alan adları resmi API'yle uyuşmuyorsa komut modu hiç çalışmaz. Önceki bloker (canlı API doğrulaması) kapatılmadan bu yapı üzerine yeni özellik inşa edilmiş; doğrulama artık iki akışı da kapsamalı.
  - [ÖNERİ] Yeni komut şeması `result_type`'ı sadece `"command"` ile sınırlıyor; eski prompt `dictation|command|wakeword|sleep` dönebiliyordu. Sonuç: (a) komut olmayan konuşma artık `action="unknown"`a zorlanıyor ve `commander::execute_command` bunu `Err("Bilinmeyen komut: unknown")` ile karşılıyor — kullanıcı anlamsız bir "Komut hatası" bildirimi görüyor. `action == "unknown"` durumu commander'a gitmeden yakalanıp "Komut anlaşılamadı: <text>" gibi nazik bir bildirimle işlensin. (b) `toggle_recording`'daki `"wakeword"`/`"sleep"` dalları ölü koda dönüştü.
  - [SORU] Wakeword/sleep sonuç tipleri şemadan çıkarılınca `wakeword_enabled`/`is_active` mekanizması tek üretim noktasını kaybetti — wake word özelliği bilinçli olarak mı emekliye ayrılıyor? Öyleyse config alanları, `is_active` state'i ve ölü dallar da temizlensin; değilse şemaya geri eklenmeli.
  - [ÖNERİ] App.tsx'te `EDITING_MODE_INFO[editingMode].description` — config dosyası elle düzenlenip `editing_mode`'a beklenmedik bir değer yazılırsa (Rust tarafı serbest `String` tutuyor, `from_config` sessizce Clean'e düşüyor ama ham değer UI'ya geliyor) `undefined.description` ile arayüz çöker. Ya Rust'ta yükleme sırasında normalize edilsin ya da UI'da `EDITING_MODE_INFO[editingMode] ?? EDITING_MODE_INFO.clean` korunağı eklensin.
  - [BİLGİ] Refinement artık yalnızca Dictation modunda çalışıyor (eskiden Translate çıktısı da refinement'a giriyordu) — Türkçe odaklı refinement promptu çeviri çıktısını bozabileceğinden bu daralma isabetli görünüyor, davranış değişikliği olarak not edildi.

### 2026-07-20 — Denetim (fc0da22)
- İncelenen aralık: `761dc73..origin/gpt-revize` — 3 yeni commit: `bf0546a` (dikte kalite/gecikme ölçüm seti), `0b6911e` (sözlük v2: Groq prompt ipucu + terim koruma), `fc0da22` (main'in gpt-revize'ye merge'ü).
- Genel değerlendirme: Kapsam, kurulum notundaki [ÖNERİ] ile birebir uyumlu ("sözlük v2 + terim koruması ilk pakette") — kapsam dışına taşma yok, silinen kritik kod yok. Merge `fc0da22` yalnızca GOZETMEN_NOTLARI.md'yi kapsıyor ve önceki [ÖNERİ] doğrultusunda main'in denetim kayıtları esas alınıp yalnızca `[ÇÖZÜLDÜ - 5b1ec7e]` satırı eklenmiş — doğru çözümleme. Derleme riski düşük: `regex` zaten bağımlılıktı, yeni fonksiyonlar mevcut importlarla uyumlu, testler eklenmiş (cargo check çalıştırılmadı, diff üzerinden değerlendirildi). Olumlu tespitler: `apply_dictionary_terms` sınır kontrolü (`[^\p{L}\p{N}_]`) alt dizeleri bozmuyor ("milonga" testi mevcut), Türkçe `ı`/`i` ayrımı test edilmiş, varyantlar uzundan kısaya sıralanıp önce takma adlar deneniyor; sözlük düzeltmesi hem Groq sonrası hem Gemini/refinement sonrası uygulanıyor; giriş boyutu sınırlandırılmış (prompt 40, değiştirme 100 kayıt); `results.jsonl` gitignore'a eklenmiş; değerlendirme betiği kelime bazlı WER + P50/P95 + terim koruma ölçüyor ve girdiyi doğruluyor; korpus 40 vaka, kimlikler benzersiz, kategoriler dengeli.
- Bulgular:
  - [ÖNERİ] `apply_dictionary_terms` regex'i sınır karakterini TÜKETIYOR (yakalama grubu 2). Ardışık iki sözlük terimi arasında tek ayraç karakteri varsa ("milov milov") ilk eşleşme aradaki boşluğu yuttuğu için ikincisi eşleşmiyor → "Millow milov". regex crate lookaround desteklemediği için ya değiştirme sonrası ikinci bir geçiş yapılsın ya da eşleşme bitişinden bir karakter geriden taramaya devam eden manuel döngü kullanılsın.
  - [ÖNERİ] Groq yolu yeni `dictionary_prompt` (yalnızca kanonik terimler) kullanırken Gemini refinement ve doğrudan Gemini promptları hâlâ ham `ctx.dictionary.join(", ")` gönderiyor — pipe'lı yeni biçimde model "Millow | milov, milo, Nietzsche | niçe" gibi belirsiz bir liste görüyor ve takma adların çıktıya sızma riski var. Gemini promptlarında da ya kanonik liste ya da açık "yanlış→doğru" eşleme biçimi kullanılsın.
  - [ÖNERİ] Whisper prompt'unun etkili uzunluğu sınırlı (~224 token); 40 kanonik terim eklenince ipucunun sonu sessizce kırpılabilir. Groq ipucu için sayı daha muhafazakâr tutulsun (örn. 15-20) veya en uzun/en kritik terimler önceliklendirilsin.
  - [ÖNERİ] `(?i)` basit case folding'de Türkçe noktalı büyük İ (U+0130) küçük `i` ile eşleşmez: ASR "MİLOV" ya da cümle başında "İstanbulkart" gibi noktalı-İ'li biçimler yakalanmaz. Pratik çözüm: kullanıcıya UI yardım metninde takma adları duyulduğu gibi küçük harfle girmesi söyleniyor zaten; ek olarak eşleşme öncesi `İ→i`, `I→ı` ön-normalizasyonu değerlendirilsin.
  - [BİLGİ] Önceki iki [BLOKER] (`apply_spoken_format_commands` tek kelimelik kalıplar; `responseFormat` alan adlarının canlı API doğrulaması) bu aralıkta ele alınmadı ve AÇIK durumda — bu paket onların üzerine yeni özellik eklemiyor ama merge öncesi çözülmeleri gerekiyor.
  - [BİLGİ] Benchmark protokolü aynı ses dosyasını üç modda koşturarak mikrofon değişkenliğini elimine ediyor — yöntem doğru. `formatting` vakalarının tam eşleşme ölçütü satır sonu normalizasyonu içeriyor; eksik koşum raporu 120 (40×3) beklentisine göre; sorun görülmedi.

---

Son denetlenen: fc0da22
