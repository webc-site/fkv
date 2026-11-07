# Mabalhin nga mga tab

- Ang pinili nga kahimtang sa label hapsay nga gisagol sa sulud sa sulud sa ubos.
- Mga detalye sa istruktura: Ang mga elemento sa bata gitakda aron magamit ang `nav > a` ingon nabigasyon sa label, ug ang direkta nga elemento sa bata `b` gigamit ingon lugar sa sulud.
- Awtomatikong i-associate ang `a[value]` tag sa `b[slot]` content node.
- Awtomatikong nagsuporta sa pagpadayon sa kasamtangan nga aktibo nga panid ngadto sa `localStorage` pinaagi sa `key` attribute.
- Nagsuporta sa pagpiho sa gi-aktibo nga panid sa tab pinaagi sa `value` attribute (kung walay cache ug `value`, ang tag nga adunay `class="A"` o ang unang tag ang pilion pinaagi sa default).
- Ang Lightweight Light DOM nga pagpatuman, walay Shadow DOM obstruction, taas nga lebel sa kagawasan sa pag-customize sa estilo.

## Gamita ang demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">mansanas</a>
    <a value="banana">saging</a>
    <a value="orange">tangerine</a>
  </nav>
  <b slot="apple">Ang mansanas usa ka masustansya nga prutas</b>
  <b slot="banana">Ang saging usa ka tropikal nga prutas</b>
  <b slot="orange">Ang mga kahel dato sa bitamina C</b>
</c-tab>
```

## kabtangan

- `key`: yawe nga ngalan nga gigamit alang sa pagpadayon sa localStorage
- `value`: karon pinili nga bili

## Pasadya nga mga panghitabo

- `change`: Na-trigger kon ang tab gibalhin, `e.value` mao ang `value` value sa kasamtangang gi-activate nga tag.