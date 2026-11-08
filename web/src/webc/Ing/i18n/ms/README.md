# Penunjuk status pemuatan atau pemprosesan

- Satukan kursor ke dalam keadaan menunggu
- Menunjukkan animasi riak untuk menunjukkan status pemprosesan
- Gunakan topeng kabur pada borang untuk mengelakkan penyerahan pendua

## Gunakan demo

```html
<!-- penunjuk pemuatan bebas -->
<div class="Ing"></div>

<!-- Memuatkan borang dengan topeng -->
<form class="Ing">
  <input type="text" placeholder="Kotak masukan">
  <button type="submit">serahkan</button>
</form>
```

## kelas gaya

### `.Ing`
- Status penunjuk ialah `wait`.
- Elemen pseudo `::before` menghasilkan animasi riak.
- Apabila digunakan pada elemen `form`, elemen pseudo `::after` menghasilkan topeng kabur 1px, melumpuhkan interaksi dan pemilihan teks.