# Byttbare faner

- Den valgte tilstanden til etiketten er sømløst integrert med innholdsområdet nedenfor.
- Strukturelle spesifikasjoner: De underordnede elementene er fikset til å bruke `nav > a` som etikettnavigering, og det direkte underordnede elementet `b` brukes som innholdsområde.
- Knytt automatisk `a[value]`-taggen til `b[slot]` innholdsnoden.
- Støtter automatisk å opprettholde gjeldende aktive side til `localStorage` gjennom `key`-attributtet.
- Støtter spesifikasjon av den aktiverte fanesiden gjennom `value`-attributtet (hvis det ikke er noen hurtigbuffer og `value`, vil taggen med `class="A"` eller den første taggen velges som standard).
- Lettvekts Light DOM-implementering, ingen Shadow DOM-hindringer, høy grad av frihet i stiltilpasning.

## Bruk demoen

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">eple</a>
    <a value="banana">banan</a>
    <a value="orange">mandarin</a>
  </nav>
  <b slot="apple">Eple er en næringsrik frukt</b>
  <b slot="banana">Banan er en tropisk frukt</b>
  <b slot="orange">Appelsiner er rike på vitamin C</b>
</c-tab>
```

## eiendom

- `key`: nøkkelnavn brukt for lokal lagrings-vedholdenhet
- `value`: gjeldende valgt verdi

## Egendefinerte hendelser

- `change`: Utløses når fanen byttes, `e.value` er `value`-verdien til den aktiverte taggen.