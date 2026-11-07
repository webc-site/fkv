# Učinek osvetlitve ozadja in robov iz motnega stekla

Polprosojna obroba in notranja senca ustvarjata mikroreliefne poudarke robov.
Kot osnovna slogovna odvisnost komponent gumba in vnosnega polja.

## Uporabite demo

```html
<main class="Lg">
  <article>
    <h3>naslov</h3>
    <pre>Besedilo vsebine</pre>
  </article>
</main>
```

## stilski razred

### `.Lg`
Osnovni slog kartice. Uporabite `backdrop-filter` zameglitev, `border` mikroreliefno obrobo in `box-shadow` notranjo senco. Uporabi `svg/glass.svg#h` filter ob lebdenju.

### `main.Lg`
Vsebnik glavnega bloka strani. Zaobljeni vogali `24px` z odzivnim oblazinjenjem (`16px`, če je širina manjša od `800px`, `24px` drugače).

### `.Btn`, `.BtnC`, `.Input`
Ime razreda osnovne komponente. Po uvedbi tega sloga boste samodejno pridobili fleksibilno postavitev, hierarhična razmerja in osnovne lastnosti motnega stekla.