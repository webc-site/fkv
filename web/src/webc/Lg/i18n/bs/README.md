# Pozadina od mat stakla i efekat isticanja ivica

Poluprozirna ivica i unutrašnja senka stvaraju mikroreljefne ivice na ivicama.
Kao osnovna stilska zavisnost komponenti dugmeta i okvira za unos.

## Koristite demo

```html
<main class="Lg">
  <article>
    <h3>naslov</h3>
    <pre>Tekst sadržaja</pre>
  </article>
</main>
```

## stilska klasa

### `.Lg`
Osnovni stil kartice. Primijenite `backdrop-filter` zamućenje, `border` mikro-reljefni okvir i `box-shadow` unutrašnju sjenu. Primijenite filter `svg/glass.svg#h` pri lebdenju.

### `main.Lg`
Kontejner glavnog bloka stranice. Zaobljeni uglovi `24px` sa responzivnim paddingom (`16px` ako je širina manja od `800px`, `24px` u suprotnom).

### `.Btn`, `.BtnC`, `.Input`
Ime klase osnovne komponente. Nakon uvođenja ovog stila, automatski ćete dobiti fleksibilan raspored, hijerarhijske odnose i osnovna svojstva matiranog stakla.