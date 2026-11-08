# Render dan tampilkan demo komponen, dokumentasi, dan instruksi instalasi

- Ekstrak dan parsing judul, pendahuluan, dan konten dokumen README
- Sandbox untuk menjalankan demo interaktif komponen dan mengisolasi gaya
- Ubah blok kode dalam dokumen menjadi presentasi interaktif
- Tata letak adaptif, tampilan kolom layar lebar, tampilan peralihan tab layar sempit

## Gunakan demonya

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inisialisasi dan dapatkan antarmuka operasi
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parameter antarmuka

### `WebCDemo(el)`

Inisialisasi komponen.

- **parameter**
  - `el`:`HTMLElement`, wadah komponen.
- **Nilai pengembalian**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Menyiapkan modul demo.
  - `setMd(name, md, lang)`: Tetapkan nama komponen, konten dokumen README, dan bahasa saat ini.
  - `unMount()`: Bongkar komponen dan bersihkan pendengar acara.

### Modul demo atribut `mod`

- `CSS`:`string`, jalur style sheet komponen.
- `HTM`:`string`, template HTML komponen.
- `default`:`function(root)`, fungsi inisialisasi komponen, menerima wadah akar bayangan.

## kelas gaya

### `.M`

bidang konten.

### `.L`

Panel kiri berisi instruksi dan dokumentasi.

### `.R`

Panel kanan, berisi demo.

### `c-nav`

Bilah navigasi layar sempit.