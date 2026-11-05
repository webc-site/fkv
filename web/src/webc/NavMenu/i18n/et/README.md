# Erinevatele ekraanisuurustele kohandatud navigeerimismenüü

- Töölaud: lingid kuvatakse horisontaalsete paanidena
- Mobiiliversioon: ahendage menüünuppu, klõpsake külgriba sahtli välja libistamiseks
- Marsruudi aktiveerimine: võrrelge automaatselt praegust teed ja räsiväärtust ning lisage vastavale lingile aktiveerimisoleku klassi nimi `A`
- Automaatne sulgemine: kui mobiilterminal klõpsab lingil või külgriba välisel taustal, sulgub külgriba automaatselt.

## kasutada

```html
<c-nav-menu>
  <a href="/">esileht</a>
  <a href="/doc">dokument</a>
  <a href="/about">umbes</a>
</c-nav-menu>
```

## illustreerima

Kui komponent on lähtestatud, kui alamelemendis pole `c-side`, luuakse automaatselt mobiilimenüü nupp ja `c-side` külgriba ning kõik `a` sildid teisaldatakse külgribale.

## stiiliklass

- **A**: aktiveeritud link lisab automaatselt klassi nime `A`