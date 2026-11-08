# Mga naililipat na tab

- Ang napiling estado ng label ay walang putol na isinama sa bahagi ng nilalaman sa ibaba.
- Mga detalye sa istruktura: Ang mga elemento ng bata ay naayos upang gamitin ang `nav > a` bilang nabigasyon ng label, at ang direktang child element na `b` ay ginagamit bilang bahagi ng nilalaman.
- Awtomatikong iugnay ang `a[value]` tag sa `b[slot]` content node.
- Sinusuportahan ang awtomatikong pagpapatuloy ng kasalukuyang aktibong pahina sa `localStorage` sa pamamagitan ng attribute na `key`.
- Sinusuportahan ang pagtukoy sa pahina ng na-activate na tab sa pamamagitan ng attribute na `value` (kung walang cache at `value`, ang tag na may `class="A"` o ang unang tag ay pipiliin bilang default).
- Magaan na pagpapatupad ng Light DOM, walang sagabal na Shadow DOM, mataas na antas ng kalayaan sa pag-customize ng istilo.

## Gamitin ang demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">mansanas</a>
    <a value="banana">saging</a>
    <a value="orange">dalanghita</a>
  </nav>
  <b slot="apple">Ang mansanas ay isang masustansyang prutas</b>
  <b slot="banana">Ang saging ay isang tropikal na prutas</b>
  <b slot="orange">Ang mga dalandan ay mayaman sa bitamina C</b>
</c-tab>
```

## ari-arian

- `key`: key name na ginamit para sa localStorage persistence
- `value`: kasalukuyang napiling halaga

## Mga custom na kaganapan

- `change`: Na-trigger kapag inilipat ang tab, ang `e.value` ay ang `value` value ng kasalukuyang naka-activate na tag.