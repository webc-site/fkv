# Jendela pop-up prompt di tepi halaman

- **Panggilan fungsi**: Menyediakan antarmuka prompt normal dan kesalahan
- **Tumpukan otomatis**: Beberapa tip secara otomatis menghitung jarak dan menumpuk secara vertikal
- **Pemutusan terjadwal dan manual**: Mendukung pematian terjadwal dan manual

## Gunakan demonya

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Konten yang diminta";
});

toastErr((el) => {
  el.textContent = "Isi kesalahan";
});

toast(
  (el) => {
    el.textContent = "Tutup secara otomatis setelah 99 detik";
  },
  99
);
```

## Parameter antarmuka

### toast(render, timeout)

Membuat dan menampilkan petunjuknya.

- `render`: `(el) => void`, rendering panggilan balik, `el` adalah elemen kotak prompt.
- `timeout`: `Number`, batas waktu detik. Default 9. Atur ke 0 agar tidak mati secara otomatis.
- Nilai kembalian: elemen kotak prompt. Mendukung `el.close()` mati.

### toastErr(render, timeout)

Membuat dan menampilkan pesan kesalahan. Parameternya sama dengan `toast`, dengan kelas gaya `.ERR`.

## kelas gaya

- `.Toast`: Kelas dasar kotak prompt.
- `.ERR`: Status kesalahan.
- `.animated` / `.fadeInR` / `.fadeOutR`: transisi animasi.
- `.x`: Tombol tutup.