# Millow dikte değerlendirmesi

Bu klasördeki 40 cümle, Millow'un hızını ve yazım kalitesini aynı içerik üzerinde karşılaştırmak için hazırlanmıştır. Set; günlük dikte, dolgu kelimeleri, konuşurken kendini düzeltme, özel terimler, sayı/kod ve sesli biçim komutlarını kapsar.

## Kayıt protokolü

1. Sessiz bir odada, mikrofondan yaklaşık 30-40 cm uzakta kayıt alın.
2. `dictation-corpus.json` içindeki `read_aloud` alanını doğal hızda ve değiştirmeden okuyun.
3. Aynı ses dosyasını Hızlı, Temiz ve Yeniden Yaz modlarında çalıştırın. Böylece mikrofon ve konuşma farkı sonucu etkilemez.
4. Her sonuç için aşağıdaki JSONL biçiminde tek satır ekleyin:

```json
{"id":"daily-01","mode":"fast","text":"Bugün markete uğrayıp süt, kahve ve ekmek alacağım.","latency_ms":742}
```

Sonuç dosyasını değerlendirin:

```bash
npm run eval:dictation -- benchmarks/results.jsonl
```

Rapor; kelime hata oranını (WER), korunması gereken terimlerin doğruluğunu, sesli biçim komutlarının tam eşleşmesini ve P50/P95 gecikmesini mod bazında gösterir. `results.jsonl` kişisel kayıt sonucu olduğu için repoya eklenmemelidir.
