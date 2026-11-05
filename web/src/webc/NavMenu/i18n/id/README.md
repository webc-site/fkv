# Menu navigasi disesuaikan dengan ukuran layar yang berbeda

- Desktop: Tautan ditampilkan dalam ubin horizontal
- Versi seluler: Tutup tombol menu, klik untuk menggeser keluar laci sidebar
- Aktivasi rute: secara otomatis membandingkan jalur saat ini dan nilai hash, dan menambahkan nama kelas status aktivasi ke tautan yang cocok `A`
- Tutup secara otomatis: Ketika terminal seluler mengklik tautan atau latar belakang eksternal dari sidebar, sidebar secara otomatis ditutup.

## menggunakan

```html
<c-nav-menu>
  <a href="/">halaman depan</a>
  <a href="/doc">dokumen</a>
  <a href="/about">tentang</a>
</c-nav-menu>
```

## menjelaskan

Ketika komponen diinisialisasi, jika tidak ada `c-side` di elemen anak, tombol menu seluler dan sidebar `c-side` akan dibuat secara otomatis, dan semua tag `a` akan dipindahkan ke sidebar.

## kelas gaya

- **A**: Tautan yang diaktifkan secara otomatis akan menambahkan nama kelas `A`