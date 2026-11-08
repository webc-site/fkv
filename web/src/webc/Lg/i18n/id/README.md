# Latar belakang kaca buram dan efek sorotan tepi

Perbatasan semi-transparan dan bayangan bagian dalam menciptakan sorotan tepi mikro-relief.
Sebagai ketergantungan gaya dasar komponen tombol dan kotak input.

## Gunakan demonya

```html
<main class="Lg">
  <article>
    <h3>judul</h3>
    <pre>Teks konten</pre>
  </article>
</main>
```

## kelas gaya

### `.Lg`
Gaya dasar kartu. Terapkan `backdrop-filter` blur, `border` batas timbul mikro, dan `box-shadow` bayangan dalam. Terapkan filter `svg/glass.svg#h` saat mengarahkan kursor.

### `main.Lg`
Kontainer blok utama halaman. Sudut membulat `24px` dengan padding responsif (`16px` jika lebarnya kurang dari `800px`, `24px` sebaliknya).

### `.Btn`, `.BtnC`, `.Input`
Nama kelas komponen dasar. Setelah memperkenalkan gaya ini, Anda secara otomatis akan mendapatkan tata letak yang fleksibel, hubungan hierarki, dan sifat dasar kaca buram.