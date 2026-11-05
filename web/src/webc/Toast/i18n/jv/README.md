# Jendhela pop-up cepet ing pinggir kaca

- **Fungsi Telpon**: Nyedhiyani antarmuka pituduh normal lan kesalahan
- **Tumpukan otomatis**: Multiple tips kanthi otomatis ngetung jarak lan tumpukan vertikal
- **Scheduled lan manual shutdown**: Ndhukung jadwal lan manual shutdown

## Gunakake demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Konten cepet";
});

toastErr((el) => {
  el.textContent = "Konten kesalahan";
});

toast(
  (el) => {
    el.textContent = "Tutup kanthi otomatis sawise 99 detik";
  },
  99
);
```

## Paramèter antarmuka

### toast(render, timeout)

Nggawe lan nampilake pituduh.

- `render`: `(el) => void`, nerjemahake panggilan balik, `el` minangka unsur kothak pituduh.
- `timeout`: `Number`, wektu entek detik. Default 9. Setel menyang 0 supaya ora mati kanthi otomatis.
- Nilai bali: unsur kothak pituduh. Ndhukung `el.close()` mati.

### toastErr(render, timeout)

Nggawe lan nampilake pesen kesalahan. Parameter padha karo `toast`, kanthi kelas gaya `.ERR`.

## kelas gaya

- `.Toast`: Kelas dhasar kothak Prompt.
- `.ERR`: Status kesalahan.
- `.animated` / `.fadeInR` / `.fadeOutR`: transisi animasi.
- `.x`: Tombol tutup.