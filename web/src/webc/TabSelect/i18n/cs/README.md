# Karta s rozevíracím výběrem

- Podporuje vložení adaptivního rozevíracího pole výběru šířky (`<c-select>`) do štítku, který se zobrazí pouze při aktivaci aktuální stránky záložky.
- Strukturální specifikace: Podřízený prvek používá `nav > a` jako navigaci štítků a přímý podřízený prvek `b` slouží jako oblast obsahu.
- Automaticky přiřadí značku `a[value]`, `a` k `c-select` (na základě atributu hodnoty skupiny a hodnoty vybrané možnosti) a uzlu obsahu `b[slot]`.
- Podporuje automatické zachování aktuálně aktivované položky na `localStorage` prostřednictvím atributu `key`.
- Podporuje automatické ukládání a obnovu historických dílčích výběrů pro každou skupinu do `localStorage` pomocí `key + '/' + tab`.
- Podporuje specifikaci aktivní karty pomocí atributu `value`.

## Použijte demo

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
      <span>systém</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript je skriptovací jazyk</b>
    <b slot="rs">Rust je programovací jazyk na systémové úrovni</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS je grafický operační systém založený na Unixu</b>
    <b slot="win">Windows je operační systém vyvinutý společností Microsoft</b>
  </b>
</c-tab-select>
```

## vlastnictví

- `key`: název klíče používaný pro persistenci localStorage
- `value`: aktuálně vybraná hodnota

## Vlastní akce

- `change`: Spouští se při přepínání karet, `e.value` je hodnota `value` aktuálně aktivované možnosti.