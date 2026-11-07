# flydende boble prompt

Vis værktøjstip-oplysninger om musen, når musen holdes, eller elementet er aktiveret.

- Baseret på native Web Component, let og højtydende
- Understøtter adaptiv rumlig positionering i fire retninger (class="top/bottom/left/right")
- Den bruger en sort matteret glasteksturgradientbaggrund og hvide tegn med udsøgte fysiske kantkonturer.
- Indbygget position adaptiv beregning, justerer automatisk pop-up retningen i henhold til den resterende plads i viewporten
- Brug `slot="pop"` til at vise flydende promptindhold

## Brug demoen

```html
<!-- Beregn automatisk retning (anbefales, ingen grund til at erklære klasse) -->
<c-pophover>
  <button>automatisk</button>
  <div slot="pop">Dette er et bobletip</div>
</c-pophover>

<!-- fast retning -->
<c-pophover class="left">
  <button>Venstre</button>
  <div slot="pop">Dette er et bobletip</div>
</c-pophover>

<c-pophover class="right">
  <button>højre</button>
  <div slot="pop">Dette er et bobletip</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Ned</button>
  <div slot="pop">Dette er et bobletip</div>
</c-pophover>

<c-pophover class="top">
  <button>overlegen</button>
  <div slot="pop">Dette er et bobletip</div>
</c-pophover>
```

## CSS variabler

Brugerdefinerede stilarter understøttes via følgende CSS-variable:

| variabel navn | standardværdi | illustrere |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Mellemrummet mellem bobleværktøjstip og det udløsende element |
| `--pophover-margin` | `24px` | Viewport-kantsikkerhedsmargen under adaptiv positioneringsberegning |
| `--pophover-bg-top` | `#18181ce0` | Bubble gradient baggrund topfarve |
| `--pophover-bg-mid` | `#121215e5` | Baggrundsfarven på pilene på venstre og højre side af boblen |
| `--pophover-bg-bottom` | `#0c0c0feb` | Farven på de nederste og øvre og nedre sidepile på boblegradientbaggrunden |

## Ikon værktøjstip stil (`.Ico`)

For flydende promptknapper af ren ikontype (såsom "Rediger", "Slet" osv.), kan du anvende et samlet ikon og svævestil ved at tilføje `.Ico` klassenavnet til selve den ydre beholder eller komponent.

### Eksempel på struktur

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">redigere</div>
  </c-pophover>
</b>
```

Det grundlæggende layout, størrelse (18x18px) og svævefarveændringer er blevet forenet i `.Ico > c-pophover > a`. Udviklere behøver kun at angive den tilsvarende ikonfil separat, såsom:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```