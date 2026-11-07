# flytende boblemelding

Vis verktøytipsinformasjon når musen holdes eller elementet er aktivert.

- Basert på native Web Component, lett og høy ytelse
- Støtter adaptiv romlig posisjonering i fire retninger (class="top/bottom/left/right")
- Den bruker en svart frostet glass teksturgradientbakgrunn og hvite tegn, med utsøkte fysiske kantkonturer.
- Innebygd posisjon adaptiv beregning, justerer automatisk popup-retningen i henhold til gjenværende plass i visningsporten
- Bruk `slot="pop"` for å vise flytende ledetekstinnhold

## Bruk demoen

```html
<!-- Beregn retning automatisk (anbefalt, ingen grunn til å deklarere klasse) -->
<c-pophover>
  <button>automatisk</button>
  <div slot="pop">Dette er et bobletips</div>
</c-pophover>

<!-- fast retning -->
<c-pophover class="left">
  <button>Igjen</button>
  <div slot="pop">Dette er et bobletips</div>
</c-pophover>

<c-pophover class="right">
  <button>høyre</button>
  <div slot="pop">Dette er et bobletips</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Ned</button>
  <div slot="pop">Dette er et bobletips</div>
</c-pophover>

<c-pophover class="top">
  <button>overlegen</button>
  <div slot="pop">Dette er et bobletips</div>
</c-pophover>
```

## CSS-variabler

Egendefinerte stiler støttes via følgende CSS-variabler:

| variabelnavn | standardverdi | illustrere |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Avstanden mellom bobleverktøytipset og det utløsende elementet |
| `--pophover-margin` | `24px` | Viewport edge sikkerhetsmargin under adaptiv posisjonsberegning |
| `--pophover-bg-top` | `#18181ce0` | Toppfarge for boblegradientbakgrunn |
| `--pophover-bg-mid` | `#121215e5` | Bakgrunnsfargen til pilene på venstre og høyre side av boblen |
| `--pophover-bg-bottom` | `#0c0c0feb` | Fargen på bunn- og øvre og nedre sidepiler på boblegradientbakgrunnen |

## Ikonverktøytipsstil (`.Ico`)

For flytende ledetekstknapper av ren ikontype (som "Rediger", "Slett" osv.), kan du bruke et enhetlig ikon og en svevestil ved å legge til `.Ico` klassenavnet i selve den ytre beholderen eller komponenten.

### Eksempel på struktur

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">redigere</div>
  </c-pophover>
</b>
```

Grunnleggende layout, størrelse (18x18px) og fargeendringer for sveve er blitt forent i `.Ico > c-pophover > a`. Utviklere trenger bare å spesifisere den tilsvarende ikonfilen separat, for eksempel:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```