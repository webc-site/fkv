# Fitxa orria goitibeherako hautapenarekin

- Zabalera moldagarria goitibeherako hautapen-koadroa (`<c-select>`) txertatzea onartzen du etiketan, uneko fitxa-orrialdea aktibatuta dagoenean soilik bistaratuko dena.
- Egitura-zehaztapena: elementu seme-alabak `nav > a` erabiltzen du etiketa-nabigazio gisa, eta zuzeneko elementu seme-alabak `b` eduki-eremu gisa balio du.
- Automatikoki lotzen ditu `a[value]` etiketa, `a` `c-select` (taldearen balio-atributuan eta aukeratutako aukera-balioan oinarrituta) eta `b[slot]` eduki-nodoarekin.
- Une aktibatuta dagoen elementua `localStorage` `key` atributuaren bidez automatikoki mantentzea onartzen du.
- Talde bakoitzeko azpi-hautaketa historikoak automatikoki gordetzea eta leheneratzea onartzen du `localStorage` `key + '/' + tab` erabiliz.
- Fitxa aktiboa `value` atributuaren bidez zehaztea onartzen du.

## Erabili demoa

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>hizkuntza</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistema</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript script-lengoaia da</b>
    <b slot="rs">Rust sistema-mailako programazio-lengoaia da</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS Unix-en oinarritutako sistema eragile grafikoa da</b>
    <b slot="win">Windows Microsoft-ek garatutako sistema eragilea da</b>
  </b>
</c-tab-select>
```

## jabetza

- `key`: localStorage iraunkorrerako erabiltzen den gako-izena
- `value`: une honetan hautatutako balioa

## Ekitaldi pertsonalizatuak

- `change`: fitxa aldatzean abiarazten da, `e.value` unean aktibatuta dagoen aukeraren `value` balioa da.