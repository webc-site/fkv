# Matglasachtergrond en randmarkeringseffect

Semi-transparante rand en binnenschaduw creëren randaccenten met micro-reliëf.
Als de basisstijlafhankelijkheid van knop- en invoervakcomponenten.

## Gebruik de demo

```html
<main class="Lg">
  <article>
    <h3>titel</h3>
    <pre>Inhoud tekst</pre>
  </article>
</main>
```

## stijl klasse

### `.Lg`
Kaart basisstijl. Pas `backdrop-filter` vervaging, `border` rand met micro-reliëf en `box-shadow` binnenschaduw toe. Pas een `svg/glass.svg#h` filter toe tijdens het zweven.

### `main.Lg`
De hoofdblokcontainer van de pagina. Afgeronde hoeken `24px` met responsieve opvulling (`16px` als de breedte kleiner is dan `800px`, `24px` anders).

### `.Btn`, `.BtnC`, `.Input`
Naam van basiscomponentklasse. Na introductie van deze stijl krijgt u automatisch een flexibele indeling, hiërarchische relaties en basiseigenschappen van matglas.