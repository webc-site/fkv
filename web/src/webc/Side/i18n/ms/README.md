# Panel laci yang meluncur keluar dari sisi

- Buat struktur tatal dalaman secara automatik
- Klik kawasan luar untuk runtuh secara automatik
- Gerak isyarat leret ke kiri dan kanan untuk menutup
- Kedudukan gelongsor sisi ke arah kiri dan kanan
- Menyediakan acara terbuka dan tutup

## Gunakan demo

```html
<button class="open-btn">Buka</button>

<c-side class="right">
  <div>Kandungan laci</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## antara muka

### kaedah

- `open(el)`: Fungsi eksport, buka laci
- `close(el)`: Fungsi eksport, tutup laci
- `el.open()`: kaedah contoh, buka laci
- `el.close()`: kaedah contoh, tutup laci

### peristiwa

- `open`: Menyala apabila dihidupkan
- `close`: Menyala apabila ditutup

## kelas gaya

- `.right`: kedudukan slaid kanan keluar
- `.top`: Luncurkan kedudukan keluar
- `.bottom`: slaid keluar dari kedudukan
- (Jika tidak ditetapkan, lalainya ialah meluncur keluar ke kiri)
- `.open`: status terbuka
- `.H`: keadaan tersembunyi
- `.T`: keadaan peralihan animasi