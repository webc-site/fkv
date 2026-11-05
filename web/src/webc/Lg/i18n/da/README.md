# Frostet glas baggrund og kant fremhæve effekt

Semi-transparent kant og indre skygge skaber mikrorelief kanthøjlys.
Som den grundlæggende stilafhængighed af knap- og inputbokskomponenter.

## Brug demoen

```html
<main class="Lg">
  <article>
    <h3>titel</h3>
    <pre>Indholdstekst</pre>
  </article>
</main>
```

## stil klasse

### `.Lg`
Kort grundlæggende stil. Påfør `backdrop-filter` sløring, `border` mikropræget kant og `box-shadow` indre skygge. Anvend `svg/glass.svg#h` filter ved svævning.

### `main.Lg`
Sidens hovedblokcontainer. Afrundede hjørner `24px` med responsiv polstring (`16px` hvis bredden er mindre end `800px`, `24px` ellers).

### `.Btn`, `.BtnC`, `.Input`
Grundlæggende komponentklassenavn. Efter at have introduceret denne stil, vil du automatisk opnå et fleksibelt layout, hierarkiske forhold og grundlæggende egenskaber ved matteret glas.