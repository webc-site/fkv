# Secara otomatis beralih menu navigasi dan tombol kembali

- Secara otomatis membungkus elemen anak untuk menghasilkan menu navigasi dan struktur tombol kembali
- Secara otomatis melacak riwayat perutean dan menghitung jalur kembali sebelumnya
- Tampilkan menu navigasi ketika rute cocok dengan item menu, jika tidak, tampilkan tombol kembali

## Gunakan demonya

```html
<c-nav-l>
  <a href="/">halaman depan</a>
  <a href="/doc">dokumen</a>
</c-nav-l>
```

## kelas gaya

- `B`: mengaktifkan status tombol kembali. Tampilkan tombol kembali dan sembunyikan menu navigasi. Secara otomatis beralih dalam komponen berdasarkan perutean.