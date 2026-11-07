# Flipasíða með fellivalmynd

- Styður innfellingu valreits fyrir aðlögunarbreidd (`<c-select>`) í merkimiðanum, sem mun aðeins birtast þegar núverandi flipasíða er virkjuð.
- Byggingarforskrift: Undireiningin notar `nav > a` sem merkimiðaleiðsögn og beineiningin `b` þjónar sem innihaldssvæði.
- Tengir sjálfkrafa `a[value]` merkið, `a` við `c-select` (byggt á gildiseiginleika hópsins og valið valkostagildi) og `b[slot]` innihaldshnútinn.
- Styður sjálfkrafa viðvarandi hlutinn sem er virkjaður í `localStorage` í gegnum `key` eigindina.
- Styður sjálfvirka vistun og endurheimt á sögulegum undirvali fyrir hvern hóp til `localStorage` með `key + '/' + tab`.
- Styður að tilgreina virka flipann í gegnum `value` eigindina.

## Notaðu kynninguna

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>tungumál</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>kerfi</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript er forskriftarmál</b>
    <b slot="rs">Rust er forritunarmál á kerfisstigi</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS er Unix byggt grafískt stýrikerfi</b>
    <b slot="win">Windows er stýrikerfi þróað af Microsoft</b>
  </b>
</c-tab-select>
```

## eign

- `key`: lykilheiti notað fyrir staðbundið geymsluþol
- `value`: valið gildi

## Sérsniðnir viðburðir

- `change`: Kveikt þegar skipt er um flipa, `e.value` er `value` gildi þess valkosts sem er virkjaður.