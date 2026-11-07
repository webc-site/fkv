# zwevende bel-prompt

Toon tooltip-informatie wanneer de muis beweegt of het element wordt geactiveerd.

- Gebaseerd op native webcomponent, lichtgewicht en krachtige prestaties
- Ondersteunt adaptieve ruimtelijke positionering in vier richtingen (class="top/bottom/left/right")
- Het maakt gebruik van een zwarte matglastextuurgradiëntachtergrond en witte karakters, met prachtige fysieke randcontouraccenten.
- Ingebouwde positie-adaptieve berekening, past automatisch de pop-uprichting aan op basis van de resterende ruimte in de viewport
- Gebruik `slot="pop"` om zwevende promptinhoud weer te geven

## Gebruik de demo

```html
<!-- Bereken automatisch de richting (aanbevolen, klasse hoeft niet te worden aangegeven) -->
<c-pophover>
  <button>automatisch</button>
  <div slot="pop">Dit is een bellentip</div>
</c-pophover>

<!-- vaste richting -->
<c-pophover class="left">
  <button>Links</button>
  <div slot="pop">Dit is een bellentip</div>
</c-pophover>

<c-pophover class="right">
  <button>rechts</button>
  <div slot="pop">Dit is een bellentip</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Omlaag</button>
  <div slot="pop">Dit is een bellentip</div>
</c-pophover>

<c-pophover class="top">
  <button>superieur</button>
  <div slot="pop">Dit is een bellentip</div>
</c-pophover>
```

## CSS-variabelen

Aangepaste stijlen worden ondersteund via de volgende CSS-variabelen:

| variabele naam | standaardwaarde | illustreren |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | De ruimte tussen de bel-tooltip en het triggerende element |
| `--pophover-margin` | `24px` | Veiligheidsmarge van de viewportrand tijdens adaptieve positioneringsberekening |
| `--pophover-bg-top` | `#18181ce0` | Achtergrondkleur van de belgradiënt |
| `--pophover-bg-mid` | `#121215e5` | De achtergrondkleur van de pijlen aan de linker- en rechterkant van de bel |
| `--pophover-bg-bottom` | `#0c0c0feb` | De kleur van de onderste en bovenste en onderste zijpijlen van de belgradiëntachtergrond |

## Tooltip-stijl van pictogram (`.Ico`)

Voor zwevende promptknoppen van het zuivere pictogramtype (zoals "Bewerken", "Verwijderen", etc.) kunt u een uniform pictogram en hoverstijl toepassen door de klassenaam `.Ico` toe te voegen aan de buitenste container of component zelf.

### Structuur voorbeeld

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">bewerking</div>
  </c-pophover>
</b>
```

De basislay-out, grootte (18x18px) en wijzigingen in de kleur van de hover zijn verenigd in `.Ico > c-pophover > a`. Ontwikkelaars hoeven alleen het bijbehorende pictogrambestand afzonderlijk op te geven, zoals:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```