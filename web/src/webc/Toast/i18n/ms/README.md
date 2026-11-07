# Tetingkap pop timbul gesaan di tepi halaman

- **Panggilan fungsi**: Menyediakan antara muka gesaan biasa dan ralat
- **Auto tindanan**: Berbilang petua mengira jarak dan tindanan secara menegak secara automatik
- **Penutupan berjadual dan manual**: Menyokong penutupan berjadual dan manual

## Gunakan demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Kandungan segera";
});

toastErr((el) => {
  el.textContent = "Kandungan ralat";
});

toast(
  (el) => {
    el.textContent = "Tutup secara automatik selepas 99 saat";
  },
  99
);
```

## Parameter antara muka

### toast(render, timeout)

Buat dan paparkan gesaan.

- `render`: `(el) => void`, membuat panggilan balik, `el` ialah elemen kotak gesaan.
- `timeout`: `Number`, saat tamat masa. Lalai 9. Tetapkan kepada 0 untuk tidak dimatikan secara automatik.
- Nilai pulangan: elemen kotak gesaan. Menyokong `el.close()` dimatikan.

### toastErr(render, timeout)

Cipta dan paparkan mesej ralat. Parameter adalah sama seperti `toast`, dengan kelas gaya `.ERR`.

## kelas gaya

- `.Toast`: Kelas asas kotak gesaan.
- `.ERR`: Status ralat.
- `.animated` / `.fadeInR` / `.fadeOutR`: peralihan animasi.
- `.x`: Butang tutup.