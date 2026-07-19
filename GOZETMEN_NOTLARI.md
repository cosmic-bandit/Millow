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

---

Son denetlenen: 3346848
