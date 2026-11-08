# Vahekaart rippmenüü valikuga

- Toetab sildile adaptiivse laiuse rippmenüü valikukasti (`<c-select>`) manustamist, mis kuvatakse ainult siis, kui praegune vaheleht on aktiveeritud.
- Struktuurispetsifikatsioon: alamelement kasutab sildi navigeerimisena elementi `nav > a` ja otsene alamelement `b` toimib sisualana.
- Seob automaatselt märgendi `a[value]`, `a` `c-select` (rühma väärtuse atribuudi ja valitud valiku väärtuse alusel) ja `b[slot]` sisusõlmega.
- Toetab praegu aktiveeritud üksuse automaatset säilitamist `localStorage` atribuudi `key` kaudu.
- Toetab iga rühma ajalooliste alamvalikute automaatset salvestamist ja taastamist `localStorage` kasutades `key + '/' + tab`.
- Toetab aktiivse vahekaardi määramist atribuudi `value` kaudu.

## Kasutage demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>keel</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>süsteem</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript on skriptikeel</b>
    <b slot="rs">Rust on süsteemitaseme programmeerimiskeel</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS on Unixi-põhine graafiline operatsioonisüsteem</b>
    <b slot="win">Windows on Microsofti välja töötatud operatsioonisüsteem</b>
  </b>
</c-tab-select>
```

## vara

- `key`: kohaliku salvestusruumi püsivuse tagamiseks kasutatav võtme nimi
- `value`: praegu valitud väärtus

## Kohandatud sündmused

- `change`: käivitatakse vahekaartide vahetamisel, `e.value` on hetkel aktiveeritud valiku `value` väärtus.