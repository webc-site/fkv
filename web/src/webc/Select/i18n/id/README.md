# Pilihan drop-down, lebar menyesuaikan dengan item saat ini

Lebar `select` asli ditentukan oleh teks opsi terpanjang secara default. Saat mengganti opsi pendek, terdapat terlalu banyak ruang putih, yang memengaruhi tampilan.

Komponen ini secara transparan mencakup `select` asli di atas tag `b`, sehingga lebarnya dapat disesuaikan secara akurat dengan panjang teks yang dipilih saat ini.

- Pertahankan gaya menu tarik-turun `select` asli dan dukungan aksesibilitas di semua terminal
- Mendukung operasi interaktif asli seperti navigasi tab keyboard lengkap dan peralihan tombol arah.
- Sesuaikan lebar dengan opsi saat ini untuk menghilangkan ruang putih berlebih

## Gunakan demonya

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```