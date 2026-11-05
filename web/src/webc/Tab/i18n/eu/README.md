# Fitxa aldagarriak

- Hautatutako etiketaren egoera ezin hobeto integratzen da beheko eduki-eremuarekin.
- Egitura-zehaztapenak: elementu seme-alabak `nav > a` etiketa-nabigazio gisa erabiltzeko finkatuta daude, eta zuzeneko elementu seme-alaba `b` eduki-eremu gisa erabiltzen da.
- Lotu `a[value]` etiketa `b[slot]` eduki-nodoarekin automatikoki.
- Uneko orrialde aktiboa `localStorage` `key` atributuaren bidez automatikoki mantentzea onartzen du.
- Aktibatutako fitxa orria `value` atributuaren bidez zehaztea onartzen du (cacherik eta `value` ez badago, `class="A"` duen etiketa edo lehen etiketa hautatuko da lehenespenez).
- DOM arina ezartzea, Shadow DOM oztoporik gabe, askatasun maila handia estilo pertsonalizazioan.

## Erabili demoa

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">sagarra</a>
    <a value="banana">platanoa</a>
    <a value="orange">mandarina</a>
  </nav>
  <b slot="apple">Sagarra fruta nutritiboa da</b>
  <b slot="banana">Banana fruta tropikala da</b>
  <b slot="orange">Laranjak C bitaminan aberatsak dira</b>
</c-tab>
```

## jabetza

- `key`: localStorage iraunkorrerako erabiltzen den gako-izena
- `value`: une honetan hautatutako balioa

## Ekitaldi pertsonalizatuak

- `change`: fitxa aldatzen denean abiarazten da, `e.value` unean aktibatuta dagoen etiketaren `value` balioa da.