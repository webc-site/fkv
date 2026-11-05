# Parsing dan render teks Markdown dengan aman ke HTML.

- **Sanitasi Keamanan**: Bertahan dari serangan XSS menggunakan `setHTML` API asli atau `DOMParser` pembersih
- **Ekstraksi Konten**: Secara otomatis membaca konten teks slot default selama inisialisasi dan merendernya
- **Pengaturan huruf bawaan**: daftar preset, tabel, blok kode, kutipan, dan gaya kotak peringatan

## Gunakan demonya

### rendering statis

```html
<c-md>
# judul
Ini adalah bagian dari konten penurunan harga
</c-md>
```

### penugasan dinamis

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Judul baru\nKonten baru";
</script>
```

## Parameter antarmuka

Teruskan teks Markdown langsung dalam tag (sebagai slot default), atau perbarui secara dinamis melalui `value`.

## Deskripsi gaya

Kelas gaya `.Md` secara otomatis ditambahkan ketika komponen diinisialisasi. Mendukung tata letak kotak peringatan perluasan Markdown berikut (dipicu oleh sintaks referensi):

- `[!NOTE]`：Informasi cepat
- `[!TIP]`: Saran/Tips
- `[!IMPORTANT]`: Perhatian penting
- `[!WARNING]`: Peringatan risiko
- `[!CAUTION]`: Peringatan risiko sangat tinggi

## mekanisme keamanan

Saat merender, panggil API `setHTML` asli terlebih dahulu untuk memasukkan HTML dengan aman. Jika browser tidak mendukungnya, browser akan secara otomatis menurunkan versi ke pembersih rekursif berdasarkan `DOMParser` dan menerapkan kebijakan keamanan berikut:
- Hapus tag yang tidak aman (misalnya `script`, `iframe`, dll.)
- Hapus `on*` atribut acara
- Pertahankan diri dari serangan XSS dengan memfilter atribut yang dimulai dengan protokol berbahaya seperti `javascript:`, `vbscript:`, dan `data:` (kecuali `data:image/`).