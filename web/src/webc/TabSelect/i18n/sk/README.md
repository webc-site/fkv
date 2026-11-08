# Karta s rozbaľovacím výberom

- Podporuje vloženie rozbaľovacieho poľa s adaptívnou šírkou (`<c-select>`) do štítka, ktoré sa zobrazí iba vtedy, keď je aktivovaná aktuálna stránka karty.
- Štrukturálna špecifikácia: Podradený prvok používa `nav > a` ako navigáciu štítkov a priamy podradený prvok `b` slúži ako oblasť obsahu.
- Automaticky priradí značku `a[value]`, `a` k `c-select` (na základe atribútu hodnoty skupiny a hodnoty vybratej možnosti) a uzlu obsahu `b[slot]`.
- Podporuje automatické zachovanie aktuálne aktivovanej položky na `localStorage` prostredníctvom atribútu `key`.
- Podporuje automatické ukladanie a obnovenie historických podvýberov pre každú skupinu na `localStorage` pomocou `key + '/' + tab`.
- Podporuje špecifikáciu aktívnej karty prostredníctvom atribútu `value`.

## Použite demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>jazyk</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>systému</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript je skriptovací jazyk</b>
    <b slot="rs">Rust je programovací jazyk na systémovej úrovni</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS je grafický operačný systém založený na Unixe</b>
    <b slot="win">Windows je operačný systém vyvinutý spoločnosťou Microsoft</b>
  </b>
</c-tab-select>
```

## majetku

- `key`: názov kľúča používaný pre trvalosť localStorage
- `value`: aktuálne vybratá hodnota

## Vlastné udalosti

- `change`: Spustí sa pri prepínaní kariet, `e.value` je hodnota `value` aktuálne aktivovanej možnosti.