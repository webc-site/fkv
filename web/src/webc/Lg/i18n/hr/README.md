# Pozadina od mat stakla i efekt isticanja rubova

Poluprozirni rub i unutarnja sjena stvaraju mikroreljefne rubove.
Kao osnovna stilska ovisnost komponenti gumba i okvira za unos.

## Koristite demo

```html
<main class="Lg">
  <article>
    <h3>titula</h3>
    <pre>Tekst sadržaja</pre>
  </article>
</main>
```

## stilska klasa

### `.Lg`
Osnovni stil kartice. Primijenite `backdrop-filter` zamućenje, `border` mikroreljefni obrub i `box-shadow` unutarnju sjenu. Primijeni `svg/glass.svg#h` filtar dok lebdiš.

### `main.Lg`
Spremnik glavnog bloka stranice. Zaobljeni kutovi `24px` s responzivnim ispunama (`16px` ako je širina manja od `800px`, `24px` inače).

### `.Btn`, `.BtnC`, `.Input`
Naziv klase osnovne komponente. Nakon uvođenja ovog stila, automatski ćete dobiti fleksibilan raspored, hijerarhijske odnose i osnovna svojstva mat stakla.