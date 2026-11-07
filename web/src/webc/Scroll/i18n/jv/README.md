# gulung bar virtual

- **Gulung rong arah**: Nyedhiyani gulung vertikal (`c-vs`) lan horisontal (`c-hs`).
- ** Tanggepan ukuran **: Ngawasi owah-owahan isi lan ukuran area, nganyari ukuran lan posisi bar gulung kanthi otomatis, lan ndhelikake kanthi otomatis nalika ora ana kebanjiran.
- ** Dhukungan interaktif **: Ndhukung nggulung roda, nyeret panggeser lan posisi klik trek.
- **Status Singidaken**: Tampilake lan ambane panggeser nalika nglayang utawa nggulung, lan kanthi otomatis ndhelikake nalika stasioner.

## gulungan vertikal

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

## gulung horisontal

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

## bagean gaya

Kustomisasi gaya elemen internal Shadow DOM liwat `::part()`:

- `::part(scroll)`: Gulung area isi.
- `::part(bar)`: Jalur gulung.
- `::part(si)`: Panggeser gulir bar.
- `::part(i)`: Lapisan pembungkus internal.
- `::part(drag)`: Trek garis gulung ing status nyeret.

## kelas gaya

- `.drag`: Ditrapake menyang `body` unsur, nuduhake yen ana ing negara seret-nggulung.

## Sifat khusus

- `--si-bg`: Warna latar mburi slider.
- `--si-anim`: Animasi panggeser.
- `--cursorScrollhSvg`: Kursor mouse nalika garis gulung horisontal dilayang.
- `--cursorScrollvSvg`: Kursor mouse nalika garis gulung vertikal dialihake.