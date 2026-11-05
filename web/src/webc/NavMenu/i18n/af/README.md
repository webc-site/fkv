# Navigasiekieslys aangepas vir verskillende skermgroottes

- Werkskerm: Skakels word in horisontale teëls vertoon
- Mobiele weergawe: Vou in kieslysknoppie in, klik om die sybalklaai uit te skuif
- Roete-aktivering: vergelyk outomaties die huidige pad en hash-waarde, en voeg aktiveringstatusklasnaam by die ooreenstemmende skakel `A`
- Maak outomaties toe: Wanneer die mobiele terminaal op 'n skakel of die eksterne agtergrond van die sidebar klik, maak die sidebar outomaties toe.

## gebruik

```html
<c-nav-menu>
  <a href="/">voorblad</a>
  <a href="/doc">dokument</a>
  <a href="/about">oor</a>
</c-nav-menu>
```

## illustreer

Wanneer die komponent geïnisialiseer is, as daar geen `c-side` in die kinderelement is nie, sal die mobiele kieslysknoppie en `c-side`-sybalk outomaties geskep word, en alle `a`-merkers sal na die sybalk geskuif word.

## styl klas

- **A**: Die geaktiveerde skakel sal outomaties die klasnaam `A` byvoeg