# Panel laci sing geser metu saka sisih

- Nggawe struktur gulung internal kanthi otomatis
- Klik area njaba kanggo ambruk kanthi otomatis
- Gesek sikep ngiwa lan nengen kanggo nutup
- Posisi ngusapake sisih ing arah kiwa lan tengen
- Nyedhiyani acara mbukak lan nutup

## Gunakake demo

```html
<button class="open-btn">Mbukak</button>

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

### cara

- `open(el)`: Fungsi ekspor, mbukak laci
- `close(el)`: Fungsi ekspor, tutup laci
- `el.open()`: metode conto, mbukak laci
- `el.close()`: metode conto, tutup laci

### acara

- `open`: Mudhun nalika diuripake
- `close`: Mudhun nalika ditutup

## kelas gaya

- `.right`: posisi geser tengen metu
- `.top`: Geser metu posisi
- `.bottom`: geser metu saka posisi
- (Yen ora disetel, standar yaiku geser metu ngiwa)
- `.open`: status mbukak
- `.H`: status sing didhelikake
- `.T`: status transisi animasi