# Menghuraikan dan memaparkan teks Markdown dengan selamat kepada HTML.

- **Pembersih Keselamatan**: Pertahankan terhadap serangan XSS menggunakan pembersih `setHTML` API atau `DOMParser` asli
- **Pengeluaran Kandungan**: Membaca kandungan teks slot lalai secara automatik semasa pemulaan dan menjadikannya
- **Penetapan taip terbina dalam**: senarai pratetap, jadual, blok kod, petikan dan gaya kotak amaran

## Gunakan demo

### rendering statik

```html
<c-md>
# tajuk
Ini adalah sebahagian daripada kandungan Markdown
</c-md>
```

### tugasan dinamik

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Tajuk baharu\nKandungan baharu";
</script>
```

## Parameter antara muka

Hantar teks Markdown terus dalam teg (sebagai slot lalai), atau kemas kini secara dinamik melalui `value`.

## Penerangan gaya

Kelas gaya `.Md` ditambah secara automatik apabila komponen dimulakan. Menyokong susun atur kotak amaran lanjutan Markdown berikut (dicetuskan oleh sintaks rujukan):

- `[!NOTE]`：Maklumat segera
- `[!TIP]`: Cadangan/Petua
- `[!IMPORTANT]`: Perhatian penting
- `[!WARNING]`: Amaran risiko
- `[!CAUTION]`: Amaran berisiko sangat tinggi

## mekanisme keselamatan

Apabila memaparkan, panggil API `setHTML` asli terlebih dahulu untuk menyuntik HTML dengan selamat. Jika penyemak imbas tidak menyokongnya, ia akan menurunkan taraf secara automatik kepada pembersih rekursif berdasarkan `DOMParser` dan melaksanakan dasar keselamatan berikut:
- Alih keluar teg tidak selamat (cth. `script`, `iframe`, dsb.)
- Kosongkan `on*` atribut acara
- Pertahankan daripada serangan XSS dengan menapis atribut bermula dengan protokol berbahaya seperti `javascript:`, `vbscript:` dan `data:` (kecuali `data:image/`).