# Menu navigasi disesuaikan dengan saiz skrin yang berbeza

- Desktop: Pautan dipaparkan dalam jubin mendatar
- Versi mudah alih: Runtuhkan ke dalam butang menu, klik untuk meluncur keluar laci bar sisi
- Pengaktifan laluan: bandingkan laluan semasa dan nilai cincang secara automatik dan tambahkan nama kelas status pengaktifan pada pautan yang sepadan `A`
- Tutup secara automatik: Apabila terminal mudah alih mengklik pada pautan atau latar belakang luaran bar sisi, bar sisi ditutup secara automatik.

## guna

```html
<c-nav-menu>
  <a href="/">muka depan</a>
  <a href="/doc">dokumen</a>
  <a href="/about">kira-kira</a>
</c-nav-menu>
```

## menggambarkan

Apabila komponen dimulakan, jika tiada `c-side` dalam elemen anak, butang menu mudah alih dan `c-side` bar sisi akan dibuat secara automatik dan semua teg `a` akan dialihkan ke bar sisi.

## kelas gaya

- **A**: Pautan yang diaktifkan akan menambah nama kelas `A` secara automatik