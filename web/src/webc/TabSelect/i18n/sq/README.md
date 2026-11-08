# Faqja e skedës me përzgjedhje rënëse

- Mbështet futjen e kutisë së përzgjedhjes së lëshimit me gjerësi adaptive (`<c-select>`) në etiketë, e cila do të shfaqet vetëm kur të aktivizohet faqja aktuale e skedës.
- Specifikimi strukturor: Elementi fëmijë përdor `nav > a` si navigim të etiketës dhe elementi i drejtpërdrejtë i fëmijës `b` shërben si zonë e përmbajtjes.
- Lidh automatikisht etiketën `a[value]`, `a` me `c-select` (bazuar në atributin e vlerës së grupit dhe vlerën e opsionit të zgjedhur) dhe nyjen e përmbajtjes `b[slot]`.
- Mbështet vazhdimësinë automatike të artikullit të aktivizuar aktualisht në `localStorage` përmes atributit `key`.
- Mbështet ruajtjen dhe rivendosjen automatike të nën-zgjedhjeve historike për çdo grup në `localStorage` duke përdorur `key + '/' + tab`.
- Mbështet specifikimin e skedës aktive përmes atributit `value`.

## Përdorni demonstrimin

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>gjuha</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistemi</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript është një gjuhë skriptimi</b>
    <b slot="rs">Rust është një gjuhë programimi e nivelit të sistemit</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS është një sistem operativ grafik i bazuar në Unix</b>
    <b slot="win">Windows është një sistem operativ i zhvilluar nga Microsoft</b>
  </b>
</c-tab-select>
```

## prone

- `key`: emri kyç i përdorur për qëndrueshmërinë e memorjes lokale
- `value`: vlera e zgjedhur aktualisht

## Ngjarjet e personalizuara

- `change`: Aktivizohet gjatë ndërrimit të skedave, `e.value` është vlera `value` e opsionit të aktivizuar aktualisht.