# bilah gulir virtual

- **Pengguliran dua arah**: Menyediakan pengguliran vertikal (`c-vs`) dan horizontal (`c-hs`).
- **Respon ukuran**: Memantau perubahan ukuran konten dan area, secara otomatis memperbarui ukuran dan posisi bilah gulir, dan secara otomatis menyembunyikan ketika tidak ada luapan.
- **Dukungan interaktif**: Mendukung pengguliran roda, menyeret penggeser, dan melacak posisi klik.
- **Status Sembunyikan**: Menampilkan dan memperlebar penggeser saat melayang atau menggulir, dan secara otomatis menyembunyikannya saat tidak bergerak.

## gulir vertikal

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## gulir horizontal

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## bagian gaya

Sesuaikan gaya elemen internal Shadow DOM melalui `::part()`:

- `::part(scroll)`: Gulir area konten.
- `::part(bar)`: Jalur bilah gulir.
- `::part(si)`: Penggeser bilah gulir.
- `::part(i)`: Lapisan pembungkus internal.
- `::part(drag)`: Jalur bilah gulir dalam keadaan menyeret.

## kelas gaya

- `.drag`: Diterapkan pada elemen `body`, menunjukkan bahwa elemen tersebut berada dalam status drag-scrolling.

## Properti khusus

- `--si-bg`: Warna latar belakang penggeser.
- `--si-anim`: Animasi penggeser.
- `--cursorScrollhSvg`: Kursor mouse ketika bilah gulir horizontal diarahkan.
- `--cursorScrollvSvg`: Kursor mouse ketika bilah gulir vertikal diarahkan.