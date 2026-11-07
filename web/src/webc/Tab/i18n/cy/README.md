# Tabiau y gellir eu newid

- Mae cyflwr dethol y label wedi'i integreiddio'n ddi-dor â'r maes cynnwys isod.
- Manylebau strwythurol: Mae'r elfennau plentyn wedi'u gosod i ddefnyddio `nav > a` fel llywio'r label, a defnyddir yr elfen plentyn uniongyrchol `b` fel y maes cynnwys.
- Cysylltwch y tag `a[value]` yn awtomatig â'r nod cynnwys `b[slot]`.
- Yn cefnogi parhau'r dudalen weithredol gyfredol yn awtomatig i `localStorage` trwy'r briodwedd `key`.
- Yn cefnogi nodi'r dudalen tab wedi'i actifadu trwy'r briodwedd `value` (os nad oes storfa a `value`, bydd y tag gyda `class="A"` neu'r tag cyntaf yn cael ei ddewis yn ddiofyn).
- Gweithredu DOM Ysgafn Ysgafn, dim rhwystr DOM Cysgodol, lefel uchel o ryddid wrth addasu arddull.

## Defnyddiwch y demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">afal</a>
    <a value="banana">banana</a>
    <a value="orange">tangerine</a>
  </nav>
  <b slot="apple">Mae afal yn ffrwyth maethlon</b>
  <b slot="banana">Ffrwyth trofannol yw banana</b>
  <b slot="orange">Mae orennau yn gyfoethog o fitamin C</b>
</c-tab>
```

## eiddo

- `key`: enw allweddol a ddefnyddir ar gyfer dyfalbarhad storio lleol
- `value`: gwerth a ddewiswyd ar hyn o bryd

## Digwyddiadau personol

- `change`: Wedi'i sbarduno pan fydd y tab wedi'i switsio, `e.value` yw gwerth `value` y tag sydd wedi'i ysgogi ar hyn o bryd.