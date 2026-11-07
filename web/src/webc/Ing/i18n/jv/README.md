# Indikator status loading utawa pangolahan

- Nyawiji kursor menyang status nunggu
- Nuduhake animasi ripple kanggo nuduhake status pangolahan
- Gunakake topeng blur menyang formulir kanggo nyegah kiriman duplikat

## Gunakake demo

```html
<!-- indikator loading independen -->
<div class="Ing"></div>

<!-- Isi formulir nganggo topeng -->
<form class="Ing">
  <input type="text" placeholder="kothak input">
  <button type="submit">ngirim</button>
</form>
```

## kelas gaya

### `.Ing`
- Status pointer yaiku `wait`.
- Unsur pseudo `::before` nggawe animasi riak.
- Nalika digunakake ing elemen `form`, unsur pseudo `::after` nggawe topeng blur 1px, mateni interaksi lan pilihan teks.