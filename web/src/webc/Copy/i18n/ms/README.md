# Salin teks ke papan keratan

Klik butang untuk menyalin teks elemen yang ditentukan.

- Menyokong penggunaan rel untuk menentukan pemilih elemen untuk disalin
- Apabila tidak dinyatakan, elemen adik beradik sebelumnya disalin secara lalai.
- Kotak gesaan Pophover bersarang
- Sokongan antarabangsa

## Gunakan demo

```html
<!-- Salin elemen tertentu -->
<span id="target">Berikut ialah teks untuk disalin</span>
<c-copy rel="#target"></c-copy>

<!-- Elemen sebelumnya disalin secara lalai -->
<span>teks elemen sebelumnya</span>
<c-copy></c-copy>
```

## parameter

| harta benda | menggambarkan |
| --- | --- |
| rel | Salin pemilih elemen sasaran. Pilihan |

## gaya status

- `c-pophover.Done`: gaya kotak gesaan selepas berjaya menyalin