# Mattklaasist taust ja servade esiletõstmise efekt

Poolläbipaistev ääris ja sisemine vari loovad mikroreljeefi servade esiletõstmised.
Nupu ja sisendkasti komponentide põhistiilisõltuvusena.

## Kasutage demo

```html
<main class="Lg">
  <article>
    <h3>pealkiri</h3>
    <pre>Sisu tekst</pre>
  </article>
</main>
```

## stiiliklass

### `.Lg`
Kaardi põhistiil. Rakendage `backdrop-filter` hägusust, `border` mikroreljeefset äärist ja `box-shadow` sisemist varju. Rakendage kursoril `svg/glass.svg#h` filter.

### `main.Lg`
Lehe põhiploki konteiner. Ümardatud nurgad `24px` tundliku polsterdusega (`16px` kui laius on väiksem kui `800px`, muidu `24px`).

### `.Btn`, `.BtnC`, `.Input`
Põhikomponendi klassi nimi. Pärast selle stiili tutvustamist saate automaatselt mattklaasi paindliku paigutuse, hierarhilised suhted ja põhiomadused.