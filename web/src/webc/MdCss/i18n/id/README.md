# Gaya tipografi dioptimalkan untuk pengalaman membaca

- **Pengaturan huruf dasar**: Menyesuaikan dengan judul, paragraf, huruf tebal, miring, dicoret, kode sebaris, gambar, dan garis pemisah.
- **Animasi tautan**: Menampilkan animasi garis bawah saat melayang.
- **Daftar proyek**: Daftar tidak berurutan menggunakan poin, daftar berurutan menggunakan nomor seri latar belakang yang dibulatkan, dan mendukung penyusunan bertingkat.
- **Daftar Tugas**: Menyediakan gaya kotak centang, mendukung tugas yang harus dilakukan dan status selesai.
- **Tips Peringatan**: Mendukung lima jenis kotak peringatan: Catatan, Tip, Penting, Peringatan, dan Perhatian.
- **Tabel dan Kode**: Menyediakan penyorotan baris tabel, dan blok kode dengan bayangan dan latar belakang gradien.

## Gunakan demonya

```html
<div class="Md">
  <h1>judul</h1>
  <p>Teks paragraf dan <a href="#">tautan</a></p>
  <hr>
  <h2>Judul tingkat kedua</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Butir Tugas</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Catatan konten</p>
  </blockquote>
</div>
```

## kelas gaya

Gaya diterapkan ke elemen HTML di bawah wadah `.Md`.

### Gaya kotak peringatan

Tambahkan nama kelas berikut pada elemen `blockquote` dengan nama kelas `.q` untuk mengaktifkan kotak peringatan:

- `note`: Catatan
- `tip`: Kiat
- `important`: Penting
- `warning`: peringatan
- `caution`: Catatan