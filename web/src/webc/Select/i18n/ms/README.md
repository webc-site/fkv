# Pilihan lungsur turun, lebar menyesuaikan diri dengan item semasa

Lebar `select` asli ditentukan oleh teks pilihan terpanjang secara lalai. Apabila menukar pilihan pendek, terdapat terlalu banyak ruang putih, yang menjejaskan penampilan.

Komponen ini secara telus meliputi `select` asli di atas teg `b`, membolehkan lebar menyesuaikan dengan tepat kepada panjang teks yang dipilih pada masa ini.

- Kekalkan gaya menu lungsur turun `select` asli dan sokongan kebolehaksesan pada semua terminal
- Menyokong operasi interaktif asli seperti navigasi Tab papan kekunci lengkap dan penukaran kekunci arah.
- Sesuaikan lebar dengan pilihan semasa untuk menghapuskan lebihan ruang putih

## Gunakan demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```