# Faneside med rullemenuvalg

- Understøtter indlejring af tilpasset bredde rullemenu (`<c-select>`) i etiketten, som kun vil blive vist, når den aktuelle faneside er aktiveret.
- Strukturel specifikation: Det underordnede element bruger `nav > a` som etiketnavigation, og det direkte underordnede element `b` fungerer som indholdsområde.
- Tilknytter automatisk `a[value]` tagget, `a` med `c-select` (baseret på gruppens værdiattribut og den valgte indstillingsværdi) og `b[slot]` indholdsknuden.
- Understøtter automatisk bevaring af det aktuelt aktiverede element til `localStorage` gennem attributten `key`.
- Understøtter automatisk lagring og gendannelse af historiske undervalg for hver gruppe til `localStorage` ved hjælp af `key + '/' + tab`.
- Understøtter specificering af den aktive fane gennem attributten `value`.

## Brug demoen

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>sprog</span>
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
    <b slot="js">JavaScript er et scriptsprog</b>
    <b slot="rs">Rust er et programmeringssprog på systemniveau</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS er et Unix-baseret grafisk operativsystem</b>
    <b slot="win">Windows er et styresystem udviklet af Microsoft</b>
  </b>
</c-tab-select>
```

## ejendom

- `key`: nøglenavn, der bruges til localStorage persistens
- `value`: aktuelt valgt værdi

## Tilpassede begivenheder

- `change`: Udløses ved skift af fane, `e.value` er `value` værdien for den aktuelt aktiverede mulighed.