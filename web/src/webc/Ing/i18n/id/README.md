# Indikator status pemuatan atau pemrosesan

- Satukan kursor ke dalam status menunggu
- Menampilkan animasi riak untuk menunjukkan status pemrosesan
- Terapkan masker buram ke formulir untuk mencegah pengiriman duplikat

## Gunakan demonya

```html
<!-- indikator pemuatan independen -->
<div class="Ing"></div>

<!-- Memuat formulir dengan topeng -->
<form class="Ing">
  <input type="text" placeholder="Kotak masukan">
  <button type="submit">kirim</button>
</form>
```

## kelas gaya

### `.Ing`
- Status penunjuk adalah `wait`.
- Elemen semu `::before` menampilkan animasi riak.
- Saat digunakan pada elemen `form`, elemen semu `::after` merender topeng buram 1 piksel, menonaktifkan interaksi dan pemilihan teks.