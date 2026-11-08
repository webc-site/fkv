# Render lan nampilake demo komponen, dokumentasi, lan instruksi instalasi

- Ekstrak lan parse judhul, introduksi lan isi dokumen README
- Sandbox kanggo mbukak demo interaktif komponen lan gaya isolasi
- Ngonversi blok kode ing dokumen dadi presentasi interaktif
- Tata letak adaptif, tampilan kolom layar lebar, tampilan ganti tab layar sempit

## Gunakake demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Initialize lan entuk antarmuka operasi
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Paramèter antarmuka

### `WebCDemo(el)`

Initialize komponen.

- **parameter**
  - `el`:`HTMLElement`, wadhah komponen.
- **Nilai bali**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Nggawe modul demo.
  - `setMd(name, md, lang)`: Setel jeneng komponen, isi dokumen README lan basa saiki.
  - `unMount()`: Mbongkar komponen lan ngresiki pamiyarsa acara.

### Modul demo `mod` atribut

- `CSS`:`string`, path sheet gaya saka komponen.
- `HTM`:`string`, cithakan HTML komponen.
- `default`:`function(root)`, fungsi initialization komponen, nampa wadhah ROOT bayangan.

## kelas gaya

### `.M`

area isi.

### `.L`

Panel kiwa ngemot instruksi lan dokumentasi.

### `.R`

Panel tengen, ngemot demo.

### `c-nav`

Pandhu arah layar sempit.