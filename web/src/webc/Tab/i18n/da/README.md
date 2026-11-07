# Skiftbare faner

- Den valgte tilstand af etiketten er problemfrit integreret med indholdsområdet nedenfor.
- Strukturelle specifikationer: De underordnede elementer er fastsat til at bruge `nav > a` som etiketnavigation, og det direkte underordnede element `b` bruges som indholdsområde.
- Tilknyt automatisk `a[value]` tagget med `b[slot]` indholdsknuden.
- Understøtter automatisk bevaring af den aktuelle aktive side til `localStorage` gennem attributten `key`.
- Understøtter specificering af den aktiverede faneside gennem attributten `value` (hvis der ikke er nogen cache og `value`, vil tagget med `class="A"` eller det første tag blive valgt som standard).
- Letvægts Light DOM implementering, ingen Shadow DOM obstruktion, høj grad af frihed i stiltilpasning.

## Brug demoen

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">æble</a>
    <a value="banana">banan</a>
    <a value="orange">mandarin</a>
  </nav>
  <b slot="apple">Æble er en nærende frugt</b>
  <b slot="banana">Banan er en tropisk frugt</b>
  <b slot="orange">Appelsiner er rige på C-vitamin</b>
</c-tab>
```

## ejendom

- `key`: nøglenavn, der bruges til localStorage persistens
- `value`: aktuelt valgt værdi

## Tilpassede begivenheder

- `change`: Udløses, når fanen skiftes, `e.value` er `value` værdien af ​​det aktuelt aktiverede tag.