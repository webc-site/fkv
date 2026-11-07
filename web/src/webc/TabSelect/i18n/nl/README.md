# Tabblad met vervolgkeuzelijst

- Ondersteunt het insluiten van een keuzevak met adaptieve breedte-vervolgkeuzelijst (`<c-select>`) in het label, dat alleen wordt weergegeven als het huidige tabblad is geactiveerd.
- Structurele specificatie: Het onderliggende element gebruikt `nav > a` als labelnavigatie, en het directe onderliggende element `b` dient als inhoudsgebied.
- Koppelt automatisch de tag `a[value]`, `a` aan `c-select` (gebaseerd op het waardeattribuut van de groep en de geselecteerde optiewaarde) en het inhoudsknooppunt `b[slot]`.
- Ondersteunt het automatisch persistent maken van het momenteel geactiveerde item naar `localStorage` via het `key` attribuut.
- Ondersteunt het automatisch opslaan en herstellen van historische subselecties voor elke groep naar `localStorage` met behulp van `key + '/' + tab`.
- Ondersteunt het specificeren van het actieve tabblad via het `value` attribuut.

## Gebruik de demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>taal</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>systeem</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript is een scripttaal</b>
    <b slot="rs">Rust is een programmeertaal op systeemniveau</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS is een op Unix gebaseerd grafisch besturingssysteem</b>
    <b slot="win">Windows is een besturingssysteem ontwikkeld door Microsoft</b>
  </b>
</c-tab-select>
```

## eigendom

- `key`: sleutelnaam gebruikt voor localStorage-persistentie
- `value`: momenteel geselecteerde waarde

## Aangepaste evenementen

- `change`: Geactiveerd bij het schakelen tussen tabbladen, `e.value` is de `value` waarde van de momenteel geactiveerde optie.