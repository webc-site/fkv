# fljótandi kúla hvetja

Birta upplýsingar um sveima tól þegar músinni er sveiflað eða frumeiningin er virkjuð.

- Byggt á innfæddum vefþáttum, léttur og afkastamikill
- Styður aðlagandi staðsetningar í fjórar áttir (class="top/bottom/left/right")
- Það notar svartan, hrímaðan gler áferð halla bakgrunn og hvíta stafi, með stórkostlega líkamlega útlínur útlínur hápunktur.
- Innbyggður stöðuaðlögunarútreikningur, stillir sprettigluggann sjálfkrafa í samræmi við plássið sem eftir er í útsýnisglugganum
- Notaðu `slot="pop"` til að birta fljótandi boðefni

## Notaðu kynninguna

```html
<!-- Reiknaðu stefnu sjálfkrafa (ráðlagt, engin þörf á að gefa upp flokk) -->
<c-pophover>
  <button>sjálfvirkur</button>
  <div slot="pop">Þetta er kúluábending</div>
</c-pophover>

<!-- föst stefnu -->
<c-pophover class="left">
  <button>Vinstri</button>
  <div slot="pop">Þetta er kúluábending</div>
</c-pophover>

<c-pophover class="right">
  <button>rétt</button>
  <div slot="pop">Þetta er kúluábending</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Niður</button>
  <div slot="pop">Þetta er kúluábending</div>
</c-pophover>

<c-pophover class="top">
  <button>æðri</button>
  <div slot="pop">Þetta er kúluábending</div>
</c-pophover>
```

## CSS breytur

Sérsniðnar stílar eru studdar með eftirfarandi CSS breytum:

| breytu nafn | sjálfgefið gildi | sýna |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Bilið á milli kúlutóls og kveikjuþáttarins |
| `--pophover-margin` | `24px` | Öryggismörk útsýnisbrúnar við útreikning á aðlögunarstaðsetningu |
| `--pophover-bg-top` | `#18181ce0` | Bubble halli bakgrunnur efst litur |
| `--pophover-bg-mid` | `#121215e5` | Bakgrunnslitur örvarnar vinstra og hægra megin á kúlu |
| `--pophover-bg-bottom` | `#0c0c0feb` | Litur neðstu og efri og neðri hliðarörvanna á bakgrunni kúluhallans |

## Stíll táknmynda (`.Ico`)

Fyrir hreina táknmyndagerð fljótandi hvetjahnappa (eins og "Breyta", "Eyða" o.s.frv.), geturðu notað sameinað tákn og sveimstíl með því að bæta `.Ico` flokksheitinu við ytri ílátið eða íhlutinn sjálfan.

### Dæmi um uppbyggingu

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">breyta</div>
  </c-pophover>
</b>
```

Grunnskipulag, stærð (18x18px) og litabreytingar á sveimi hafa verið sameinaðar í `.Ico > c-pophover > a`. Hönnuðir þurfa aðeins að tilgreina samsvarandi táknskrá sérstaklega, svo sem:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```