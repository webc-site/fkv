# slankiojo burbulo raginimas

Rodyti žymeklio patarimo informaciją, kai užvedama pelė arba suaktyvinamas elementas.

- Pagrįstas vietiniu žiniatinklio komponentu, lengvas ir didelio našumo
- Palaiko prisitaikantį erdvinės padėties nustatymą keturiomis kryptimis (class="top/bottom/left/right")
- Jame naudojamas juodo matinio stiklo tekstūros gradiento fonas ir balti rašmenys su išskirtiniais fiziniais kraštų kontūrais.
- Integruotas padėties adaptacinis skaičiavimas, automatiškai koreguoja iššokančiojo lango kryptį pagal likusią erdvę peržiūros srityje
- Naudokite `slot="pop"`, kad būtų rodomas slankusis raginimo turinys

## Naudokite demonstracinę versiją

```html
<!-- Automatiškai apskaičiuoti kryptį (rekomenduojama, nereikia deklaruoti klasės) -->
<c-pophover>
  <button>automatinis</button>
  <div slot="pop">Tai yra burbulo antgalis</div>
</c-pophover>

<!-- fiksuota kryptis -->
<c-pophover class="left">
  <button>Kairė</button>
  <div slot="pop">Tai yra burbulo antgalis</div>
</c-pophover>

<c-pophover class="right">
  <button>teisingai</button>
  <div slot="pop">Tai yra burbulo antgalis</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Žemyn</button>
  <div slot="pop">Tai yra burbulo antgalis</div>
</c-pophover>

<c-pophover class="top">
  <button>pranašesnis</button>
  <div slot="pop">Tai yra burbulo antgalis</div>
</c-pophover>
```

## CSS kintamieji

Tinkinti stiliai palaikomi naudojant šiuos CSS kintamuosius:

| kintamasis pavadinimas | numatytoji vertė | iliustruoti |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Tarpas tarp burbulo patarimo ir paleidimo elemento |
| `--pophover-margin` | `24px` | Vaizdo srities krašto saugos riba adaptyvaus padėties nustatymo skaičiavimo metu |
| `--pophover-bg-top` | `#18181ce0` | Burbulo gradiento fono viršutinė spalva |
| `--pophover-bg-mid` | `#121215e5` | Kairėje ir dešinėje burbulo pusėse esančių rodyklių fono spalva |
| `--pophover-bg-bottom` | `#0c0c0feb` | Burbulo gradiento fono apatinės ir viršutinės bei apatinės šoninių rodyklių spalva |

## Piktogramos patarimo stilius (`.Ico`)

Jei naudojate gryno piktogramos tipo slankiuosius raginimo mygtukus (pvz., „Redaguoti“, „Ištrinti“ ir kt.), galite pritaikyti vieningą piktogramą ir užvedimo stilių, pridėdami `.Ico` klasės pavadinimą prie paties išorinio konteinerio arba komponento.

### Struktūros pavyzdys

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">redaguoti</div>
  </c-pophover>
</b>
```

Pagrindinis išdėstymas, dydis (18 x 18 piks.) ir užvedimo spalvos pakeitimai buvo suvienodinti `.Ico > c-pophover > a`. Kūrėjams tereikia atskirai nurodyti atitinkamą piktogramos failą, pvz.:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```