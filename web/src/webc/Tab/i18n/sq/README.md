# Skedat e ndërrueshme

- Gjendja e zgjedhur e etiketës është e integruar pa probleme me zonën e përmbajtjes më poshtë.
- Specifikimet strukturore: Elementet fëmijë janë fiksuar për të përdorur `nav > a` si navigim të etiketës dhe elementi i drejtpërdrejtë i fëmijës `b` përdoret si zona e përmbajtjes.
- Lidhni automatikisht etiketën `a[value]` me nyjen e përmbajtjes `b[slot]`.
- Mbështet vazhdimësinë automatike të faqes aktuale aktive në `localStorage` përmes atributit `key`.
- Mbështet specifikimin e faqes së skedës së aktivizuar përmes atributit `value` (nëse nuk ka cache dhe `value`, etiketa me `class="A"` ose etiketa e parë do të zgjidhet si parazgjedhje).
- Zbatim i lehtë DOM i lehtë, pa pengesë Shadow DOM, shkallë e lartë lirie në përshtatjen e stilit.

## Përdorni demonstrimin

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">mollë</a>
    <a value="banana">banane</a>
    <a value="orange">mandarina</a>
  </nav>
  <b slot="apple">Molla është një frut ushqyes</b>
  <b slot="banana">Banania është një frut tropikal</b>
  <b slot="orange">Portokallet janë të pasura me vitaminë C</b>
</c-tab>
```

## prone

- `key`: emri kyç i përdorur për qëndrueshmërinë e memorjes lokale
- `value`: vlera e zgjedhur aktualisht

## Ngjarjet e personalizuara

- `change`: Aktivizohet kur ndërrohet skeda, `e.value` është vlera `value` e etiketës së aktivizuar aktualisht.