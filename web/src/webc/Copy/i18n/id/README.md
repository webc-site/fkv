# Salin teks ke papan klip

Klik tombol untuk menyalin teks dari elemen yang ditentukan.

- Mendukung penggunaan rel untuk menentukan pemilih elemen yang akan disalin
- Jika tidak ditentukan, elemen saudara sebelumnya akan disalin secara default.
- Kotak prompt Pophover bersarang
- Dukungan internasional

## Gunakan demonya

```html
<!-- Salin elemen tertentu -->
<span id="target">Ini teks yang akan disalin</span>
<c-copy rel="#target"></c-copy>

<!-- Elemen sebelumnya disalin secara default -->
<span>teks elemen sebelumnya</span>
<c-copy></c-copy>
```

## parameter

| milik | menjelaskan |
| --- | --- |
| rel | Salin pemilih elemen target. Opsional |

## gaya status

- `c-pophover.Done`: gaya kotak prompt setelah penyalinan berhasil