# Panel laci yang dapat digeser keluar dari samping

- Secara otomatis membuat struktur gulir internal
- Klik area luar untuk menciutkannya secara otomatis
- Gesek gerakan ke kiri dan kanan untuk menutup
- Posisi geser samping ke arah kiri dan kanan
- Menyediakan acara terbuka dan tertutup

## Gunakan demonya

```html
<button class="open-btn">Membuka</button>

<c-side class="right">
  <div>Isi laci</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## antarmuka

### metode

- `open(el)`: Fungsi ekspor, laci terbuka
- `close(el)`: Fungsi ekspor, tutup laci
- `el.open()`: metode instan, laci terbuka
- `el.close()`: metode instan, tutup laci

### peristiwa

- `open`: Menyala saat dihidupkan
- `close`: Menyala ketika ditutup

## kelas gaya

- `.right`: posisi geser ke kanan
- `.top`: Geser keluar posisi
- `.bottom`: meluncur keluar dari posisinya
- (Jika tidak disetel, defaultnya adalah menggeser ke kiri)
- `.open`: status terbuka
- `.H`: keadaan tersembunyi
- `.T`: keadaan transisi animasi