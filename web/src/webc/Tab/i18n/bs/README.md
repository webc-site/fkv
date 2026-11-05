# Promenljive kartice

- Odabrano stanje naljepnice je neprimjetno integrirano s područjem sadržaja ispod.
- Strukturne specifikacije: podređeni elementi su fiksni da koriste `nav > a` kao navigaciju oznakama, a direktni podređeni element `b` se koristi kao područje sadržaja.
- Automatski povežite oznaku `a[value]` sa sadržajnim čvorom `b[slot]`.
- Podržava automatsko zadržavanje trenutne aktivne stranice na `localStorage` kroz atribut `key`.
- Podržava određivanje aktivirane kartice kartice putem atributa `value` (ako nema keš memorije i `value`, oznaka sa `class="A"` ili prva oznaka će biti odabrana po defaultu).
- Lagana Light DOM implementacija, bez Shadow DOM prepreka, visok stepen slobode u prilagođavanju stila.

## Koristite demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">jabuka</a>
    <a value="banana">banana</a>
    <a value="orange">mandarina</a>
  </nav>
  <b slot="apple">Jabuka je hranljivo voće</b>
  <b slot="banana">Banana je tropsko voće</b>
  <b slot="orange">Narandže su bogate vitaminom C</b>
</c-tab>
```

## imovine

- `key`: ime ključa koji se koristi za postojanost localStorage
- `value`: trenutno odabrana vrijednost

## Prilagođeni događaji

- `change`: Pokreće se kada se kartica promijeni, `e.value` je `value` vrijednost trenutno aktivirane oznake.