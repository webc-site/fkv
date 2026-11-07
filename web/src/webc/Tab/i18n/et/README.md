# Vahetatavad vahelehed

- Sildi valitud olek on sujuvalt integreeritud alloleva sisualaga.
- Struktuurilised spetsifikatsioonid: alamelemendid on fikseeritud kasutama sildi navigeerimisena `nav > a` ja sisualana kasutatakse otsest alamelementi `b`.
- Seostage märgend `a[value]` automaatselt sisusõlmega `b[slot]`.
- Toetab praeguse aktiivse lehe automaatset säilitamist `localStorage` atribuudi `key` kaudu.
- Toetab aktiveeritud vahekaardi määramist atribuudi `value` kaudu (kui vahemälu ja `value` pole, valitakse vaikimisi märgend `class="A"` või esimene silt).
- Kerge Kerge DOM-i rakendamine, Shadow DOM-i takistuseta, stiili kohandamisel on suur vabadus.

## Kasutage demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">õun</a>
    <a value="banana">banaan</a>
    <a value="orange">mandariin</a>
  </nav>
  <b slot="apple">Õun on toitev puuvili</b>
  <b slot="banana">Banaan on troopiline puuvili</b>
  <b slot="orange">Apelsinid on rikkad C-vitamiini poolest</b>
</c-tab>
```

## vara

- `key`: kohaliku salvestusruumi püsivuse tagamiseks kasutatav võtme nimi
- `value`: praegu valitud väärtus

## Kohandatud sündmused

- `change`: käivitatakse vahekaardi vahetamisel, `e.value` on hetkel aktiveeritud märgendi `value` väärtus.