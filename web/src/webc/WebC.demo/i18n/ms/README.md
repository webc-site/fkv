# Render dan paparkan demo komponen, dokumentasi dan arahan pemasangan

- Ekstrak dan huraikan tajuk, pengenalan dan kandungan dokumen README
- Kotak pasir untuk menjalankan demo interaktif komponen dan mengasingkan gaya
- Tukar blok kod dalam dokumen kepada pembentangan interaktif
- Reka letak penyesuaian, paparan lajur skrin lebar, paparan penukaran tab skrin sempit

## Gunakan demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Memulakan dan mendapatkan antara muka operasi
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parameter antara muka

### `WebCDemo(el)`

Mulakan komponen.

- **parameter**
  - `el`:`HTMLElement`, bekas komponen.
- **Nilai pulangan**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Sediakan modul demo.
  - `setMd(name, md, lang)`: Tetapkan nama komponen, kandungan dokumen README dan bahasa semasa.
  - `unMount()`: Nyahmuat komponen dan bersihkan pendengar acara.

### Atribut `mod` modul demo

- `CSS`:`string`, laluan helaian gaya komponen.
- `HTM`:`string`, templat HTML komponen.
- `default`:`function(root)`, fungsi permulaan komponen, menerima bekas akar bayangan.

## kelas gaya

### `.M`

kawasan kandungan.

### `.L`

Panel kiri mengandungi arahan dan dokumentasi.

### `.R`

Panel kanan, mengandungi demo.

### `c-nav`

Bar navigasi skrin sempit.