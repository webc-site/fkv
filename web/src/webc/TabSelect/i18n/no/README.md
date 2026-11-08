# Faneside med rullegardinvalg

- Støtter innebygging av rullegardinmenyen for adaptiv bredde (`<c-select>`) i etiketten, som bare vises når gjeldende faneside er aktivert.
- Strukturell spesifikasjon: Det underordnede elementet bruker `nav > a` som etikettnavigasjon, og det direkte underordnede elementet `b` fungerer som innholdsområdet.
- Knytter automatisk `a[value]`-taggen, `a` til `c-select` (basert på gruppens verdiattributt og den valgte alternativverdien) og `b[slot]` innholdsnoden.
- Støtter automatisk å fortsette det aktiverte elementet til `localStorage` gjennom `key`-attributtet.
- Støtter automatisk lagring og gjenoppretting av historiske undervalg for hver gruppe til `localStorage` ved hjelp av `key + '/' + tab`.
- Støtter spesifisering av den aktive fanen gjennom `value`-attributtet.

## Bruk demoen

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>språk</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>system</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript er et skriptspråk</b>
    <b slot="rs">Rust er et programmeringsspråk på systemnivå</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS er et Unix-basert grafisk operativsystem</b>
    <b slot="win">Windows er et operativsystem utviklet av Microsoft</b>
  </b>
</c-tab-select>
```

## eiendom

- `key`: nøkkelnavn brukt for lokal lagrings-vedholdenhet
- `value`: gjeldende valgt verdi

## Egendefinerte hendelser

- `change`: Utløses når du bytter fane, `e.value` er `value` verdien for det aktive alternativet.