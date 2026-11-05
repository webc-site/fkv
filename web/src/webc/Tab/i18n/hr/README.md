# Kartice koje se mogu mijenjati

- Odabrano stanje oznake neprimjetno je integrirano s područjem sadržaja ispod.
- Strukturne specifikacije: podređeni elementi su fiksirani da koriste `nav > a` kao navigaciju oznake, a izravni podređeni element `b` koristi se kao područje sadržaja.
- Automatski pridruži oznaku `a[value]` čvoru sadržaja `b[slot]`.
- Podržava automatsko zadržavanje trenutne aktivne stranice na `localStorage` putem atributa `key`.
- Podržava određivanje aktivirane stranice kartice putem atributa `value` (ako nema predmemorije i `value`, oznaka s `class="A"` ili prva oznaka bit će odabrana prema zadanim postavkama).
- Lagana Light DOM implementacija, nema Shadow DOM prepreka, visok stupanj slobode u prilagodbi stila.

## Koristite demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">jabuka</a>
    <a value="banana">banana</a>
    <a value="orange">mandarina</a>
  </nav>
  <b slot="apple">Jabuka je hranjivo voće</b>
  <b slot="banana">Banana je tropsko voće</b>
  <b slot="orange">Naranče su bogate vitaminom C</b>
</c-tab>
```

## imovine

- `key`: naziv ključa koji se koristi za postojanost localStorage
- `value`: trenutno odabrana vrijednost

## Prilagođeni događaji

- `change`: Pokreće se kada se kartica prebaci, `e.value` je `value` vrijednost trenutno aktivirane oznake.