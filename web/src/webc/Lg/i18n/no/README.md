# Frostet glassbakgrunn og kantfremhevingseffekt

Halvtransparent kant og indre skygge skaper mikrorelieff kanthøydepunkter.
Som den grunnleggende stilavhengigheten til knapp- og inndatabokskomponenter.

## Bruk demoen

```html
<main class="Lg">
  <article>
    <h3>tittel</h3>
    <pre>Innholdstekst</pre>
  </article>
</main>
```

## stilklasse

### `.Lg`
Kort grunnleggende stil. Påfør `backdrop-filter` uskarphet, `border` mikropreget kant og `box-shadow` indre skygge. Bruk `svg/glass.svg#h` filter når du holder musepekeren.

### `main.Lg`
Sidens hovedblokkbeholder. Avrundede hjørner `24px` med responsiv polstring (`16px` hvis bredden er mindre enn `800px`, `24px` ellers).

### `.Btn`, `.BtnC`, `.Input`
Grunnleggende komponentklassenavn. Etter å ha introdusert denne stilen vil du automatisk få fleksibel layout, hierarkiske relasjoner og grunnleggende egenskaper til frostet glass.