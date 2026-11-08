# kërkesë flluskë lundruese

Shfaq informacionin e këshillës së veglave të pezullimit kur miu është pezulluar ose elementi është aktivizuar.

- Bazuar në komponentin vendas të Uebit, i lehtë dhe me performancë të lartë
- Mbështet pozicionimin hapësinor adaptiv në katër drejtime (class="lart/bottom/left/djathtas")
- Ai përdor një sfond të gradientit të teksturës së xhamit të mbuluar me brymë të zezë dhe karaktere të bardha, me theksime të shkëlqyera të kontureve të skajeve fizike.
- Llogaritja adaptive e pozicionit të integruar, rregullon automatikisht drejtimin e hapjes sipas hapësirës së mbetur në portën e shikimit
- Përdorni `slot="pop"` për të shfaqur përmbajtje të shpejtë lundruese

## Përdorni demonstrimin

```html
<!-- Llogaritni automatikisht drejtimin (rekomandohet, nuk ka nevojë të deklaroni klasën) -->
<c-pophover>
  <button>automatike</button>
  <div slot="pop">Ky është një tip flluskë</div>
</c-pophover>

<!-- drejtim fiks -->
<c-pophover class="left">
  <button>Majtas</button>
  <div slot="pop">Ky është një tip flluskë</div>
</c-pophover>

<c-pophover class="right">
  <button>drejtë</button>
  <div slot="pop">Ky është një tip flluskë</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Poshtë</button>
  <div slot="pop">Ky është një tip flluskë</div>
</c-pophover>

<c-pophover class="top">
  <button>epror</button>
  <div slot="pop">Ky është një tip flluskë</div>
</c-pophover>
```

## Variablat CSS

Stilet e personalizuara mbështeten nëpërmjet variablave të mëposhtëm CSS:

| emri i ndryshores | vlera e paracaktuar | ilustrojnë |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Hapësira midis majës së veglës së flluskës dhe elementit nxitës |
| `--pophover-margin` | `24px` | Marzhi i sigurisë së skajit të pamjes gjatë llogaritjes së pozicionimit adaptiv |
| `--pophover-bg-top` | `#18181ce0` | Ngjyra e sipërme e sfondit me gradient flluskë |
| `--pophover-bg-mid` | `#121215e5` | Ngjyra e sfondit të shigjetave në anën e majtë dhe të djathtë të flluskës |
| `--pophover-bg-bottom` | `#0c0c0feb` | Ngjyra e shigjetave anësore të poshtme dhe të sipërme dhe të poshtme të sfondit të gradientit të flluskës |

## Stili i këshillës së veglave të ikonës (`.Ico`)

Për butonat lundrues të tipit të pastër të ikonës (të tilla si "Redakto", "Fshi", etj.), mund të aplikoni një ikonë të unifikuar dhe stilin e pezullimit duke shtuar emrin e klasës `.Ico` në kontejnerin ose komponentin e jashtëm.

### Shembull i strukturës

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">redaktoni</div>
  </c-pophover>
</b>
```

Struktura bazë, madhësia (18x18px) dhe ndryshimet e ngjyrës së pezullimit janë unifikuar në `.Ico > c-pophover > a`. Zhvilluesit duhet vetëm të specifikojnë veçmas skedarin përkatës të ikonës, si p.sh.
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```