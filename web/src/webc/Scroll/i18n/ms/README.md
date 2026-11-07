# bar skrol maya

- **Penatalan dua hala**: Menyediakan penatalan menegak (`c-vs`) dan mendatar (`c-hs`).
- **Saiz respons**: Pantau kandungan dan perubahan saiz kawasan, kemas kini saiz dan kedudukan bar skrol secara automatik, dan sembunyikan secara automatik apabila tiada limpahan.
- **Sokongan interaktif**: Menyokong tatal roda, menyeret gelangsar dan menjejaki kedudukan klik.
- **Status Sembunyi**: Tunjukkan dan luaskan gelangsar apabila melayang atau menatal, dan secara automatik menyembunyikannya apabila pegun.

## tatal menegak

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

## tatal mendatar

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

## bahagian gaya

Sesuaikan gaya elemen dalaman Shadow DOM melalui `::part()`:

- `::part(scroll)`: Tatal kawasan kandungan.
- `::part(bar)`: Trek bar tatal.
- `::part(si)`: Peluncur bar tatal.
- `::part(i)`: Lapisan pembalut dalaman.
- `::part(drag)`: Trek bar skrol dalam keadaan menyeret.

## kelas gaya

- `.drag`: Digunakan pada elemen `body`, menunjukkan bahawa ia berada dalam keadaan seret-menatal.

## Sifat tersuai

- `--si-bg`: Warna latar belakang peluncur.
- `--si-anim`: Animasi peluncur.
- `--cursorScrollhSvg`: Kursor tetikus apabila bar skrol mendatar dilegar.
- `--cursorScrollvSvg`: Kursor tetikus apabila bar skrol menegak dilegar.