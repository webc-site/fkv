# Latar mburi kaca es lan efek sorot pinggir

Watesan semi-transparan lan bayangan njero nggawe sorotan pinggiran mikro-relief.
Minangka gaya dhasar katergantungan tombol lan komponen kothak input.

## Gunakake demo

```html
<main class="Lg">
  <article>
    <h3>judhul</h3>
    <pre>Teks isi</pre>
  </article>
</main>
```

## kelas gaya

### `.Lg`
Gaya dhasar kertu. Gunakake `backdrop-filter` blur, `border` wates micro-embossed, lan `box-shadow` bayangan batin. Gunakake filter `svg/glass.svg#h` ing hover.

### `main.Lg`
Wadah blok utama kaca. Sudut bunder `24px` kanthi bantalan responsif (`16px` yen ambane kurang saka `800px`, `24px` liya).

### `.Btn`, `.BtnC`, `.Input`
Jeneng kelas komponen dhasar. Sawise ngenalake gaya iki, sampeyan bakal entuk tata letak fleksibel, hubungan hierarkis lan sifat dhasar kaca beku kanthi otomatis.