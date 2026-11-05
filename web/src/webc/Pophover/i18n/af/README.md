# drywende borrel-opdrag

Vertoon sweefnutspuntinligting wanneer die muis gesweef word of die element geaktiveer is.

- Gebaseer op inheemse webkomponent, liggewig en hoë werkverrigting
- Ondersteun aanpasbare ruimtelike posisionering in vier rigtings (klas="bo/onder/links/regs")
- Dit gebruik 'n swart matglas tekstuur gradiënt agtergrond en wit karakters, met pragtige fisiese rand kontoer hoogtepunte.
- Ingeboude posisie-aanpasbare berekening, pas die opspringrigting outomaties aan volgens die oorblywende spasie in die uitsigpoort
- Gebruik `slot="pop"` om drywende boodskapinhoud te vertoon

## Gebruik die demo

```html
<!-- Bereken rigting outomaties (aanbeveel, nie nodig om klas te verklaar nie) -->
<c-pophover>
  <button>outomaties</button>
  <div slot="pop">Dit is 'n borrelwenk</div>
</c-pophover>

<!-- vaste rigting -->
<c-pophover class="left">
  <button>Links</button>
  <div slot="pop">Dit is 'n borrelwenk</div>
</c-pophover>

<c-pophover class="right">
  <button>reg</button>
  <div slot="pop">Dit is 'n borrelwenk</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Af</button>
  <div slot="pop">Dit is 'n borrelwenk</div>
</c-pophover>

<c-pophover class="top">
  <button>meerderwaardig</button>
  <div slot="pop">Dit is 'n borrelwenk</div>
</c-pophover>
```

## CSS veranderlikes

Pasgemaakte style word ondersteun deur die volgende CSS-veranderlikes:

| veranderlike naam | verstekwaarde | illustreer |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Die spasie tussen die borrelnutspunt en die snellerelement |
| `--pophover-margin` | `24px` | Viewport rand veiligheidsmarge tydens aanpasbare posisionering berekening |
| `--pophover-bg-top` | `#18181ce0` | Bubble gradiënt agtergrond top kleur |
| `--pophover-bg-mid` | `#121215e5` | Die agtergrondkleur van die pyle aan die linker- en regterkant van die borrel |
| `--pophover-bg-bottom` | `#0c0c0feb` | Die kleur van die onderste en boonste en onderste sypyltjies van die borrelgradiënt-agtergrond |

## Ikoonnutspuntstyl (`.Ico`)

Vir suiwer ikoontipe drywende promptknoppies (soos "Redigeer", "Vee uit", ens.), kan jy 'n verenigde ikoon en sweefstyl toepas deur die `.Ico` klasnaam by die buitenste houer of komponent self te voeg.

### Struktuur voorbeeld

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">wysig</div>
  </c-pophover>
</b>
```

Die basiese uitleg, grootte (18x18px) en sweefkleurveranderinge is verenig in `.Ico > c-pophover > a`. Ontwikkelaars hoef slegs die ooreenstemmende ikoonlêer afsonderlik te spesifiseer, soos:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```