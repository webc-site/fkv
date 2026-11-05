# virtuali slinkties juosta

- **Dvipusis slinkimas**: galima slinkti vertikaliai (`c-vs`) ir horizontaliai (`c-hs`).
- **Dydžio atsakas**: Stebėkite turinio ir srities dydžio pokyčius, automatiškai atnaujinkite slinkties juostos dydį ir padėtį ir automatiškai slėpkite, kai nėra perpildymo.
- **Interaktyvus palaikymas**: palaiko ratuko slinkimą, slankiklio vilkimą ir takelio paspaudimo padėties nustatymą.
- **Būsenos slėpimas**: rodykite ir išplėskite slankiklį, kai užvedate pelės žymeklį arba slenkate, ir automatiškai paslėpkite jį, kai stovite.

## vertikali slinktis

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

## horizontalus slinkimas

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

## stiliaus dalys

Tinkinkite „Shadow DOM“ vidinių elementų stilių naudodami `::part()`:

- `::part(scroll)`: slinkite turinio sritimi.
- `::part(bar)`: slinkties juostos takelis.
- `::part(si)`: slinkties juostos slankiklis.
- `::part(i)`: vidinis vyniojimo sluoksnis.
- `::part(drag)`: slinkties juostos takelis vilkimo būsenoje.

## stiliaus klasė

- `.drag`: taikoma `body` elementui, nurodant, kad jis yra vilkimo slinkimo būsenoje.

## Pasirinktinės savybės

- `--si-bg`: slankiklio fono spalva.
- `--si-anim`: slankiklio animacija.
- `--cursorScrollhSvg`: pelės žymeklis, kai užvedama horizontali slinkties juosta.
- `--cursorScrollvSvg`: pelės žymeklis, kai užvedama vertikali slinkties juosta.