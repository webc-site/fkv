# schwiewend Bubble Prompt

Weist Hover Tooltip Informatioun wann d'Maus hoveréiert oder d'Element aktivéiert ass.

- Baséiert op gebierteg Web Komponent, liicht an héich-Performance
- Ënnerstëtzt adaptiv raimlech Positionéierung a véier Richtungen (Klass = Top / Bottom / Left / Right")
- Et benotzt e schwaarze frostéierte Glas Texturgradienthannergrond a wäiss Zeechen, mat exquisite kierperleche Randkontur Highlights.
- Built-in Positioun adaptiv Berechnung, passt automatesch d'Pop-up Richtung no de verbleiwen Raum am Viewport un
- Benotzt `slot="pop"` fir schwiewend Prompt Inhalt ze weisen

## Benotzt d'Demo

```html
<!-- Automatesch Berechent Richtung (recommandéiert, net néideg Klass ze deklaréieren) -->
<c-pophover>
  <button>automatesch</button>
  <div slot="pop">Dëst ass e Bubble Tipp</div>
</c-pophover>

<!-- fix Richtung -->
<c-pophover class="left">
  <button>Lénks</button>
  <div slot="pop">Dëst ass e Bubble Tipp</div>
</c-pophover>

<c-pophover class="right">
  <button>riets</button>
  <div slot="pop">Dëst ass e Bubble Tipp</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Down</button>
  <div slot="pop">Dëst ass e Bubble Tipp</div>
</c-pophover>

<c-pophover class="top">
  <button>superior</button>
  <div slot="pop">Dëst ass e Bubble Tipp</div>
</c-pophover>
```

## CSS Variablen

Benotzerdefinéiert Stiler ginn iwwer déi folgend CSS Variablen ënnerstëtzt:

| Variabel Numm | Standardwäert | illustréieren |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | De Raum tëscht dem Bubble Tooltip an dem Ausléiserelement |
| `--pophover-margin` | `24px` | Viewport Rand Sécherheetsmarge während adaptiven Positionéierungsberechnung |
| `--pophover-bg-top` | `#18181ce0` | Bubble Gradient Hannergrond Top Faarf |
| `--pophover-bg-mid` | `#121215e5` | Den Hannergrondfaarf vun de Pfeile op der lénker a rietser Säit vun der Bubble |
| `--pophover-bg-bottom` | `#0c0c0feb` | D'Faarf vun den ënneschten an ieweschten an ënneschten Säit Pfeile vum Bubble Gradient Hannergrond |

## Ikon Tooltip Stil (`.Ico`)

Fir pure Ikon Typ schwiewend Prompt Knäppercher (wéi "Änneren", "Läschen", etc.), Dir kënnt eng vereenegt Ikon an Hover Stil applizéieren andeems Dir den `.Ico` Klassennumm un de baussenzege Container oder de Komponent selwer bäidréit.

### Struktur Beispill

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">änneren</div>
  </c-pophover>
</b>
```

D'Basis Layout, Gréisst (18x18px), an Hover Faarf Ännerungen goufen vereenegt an `.Ico > c-pophover > a`. Entwéckler brauchen nëmmen déi entspriechend Ikondatei separat ze spezifizéieren, sou wéi:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```