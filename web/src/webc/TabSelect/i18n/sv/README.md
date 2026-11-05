# Fliksida med rullgardinsval

- Stöder inbäddning av rullgardinsmenyn för adaptiv bredd (`<c-select>`) i etiketten, som endast visas när den aktuella fliksidan är aktiverad.
- Strukturell specifikation: Det underordnade elementet använder `nav > a` som etikettnavigering, och det direkta underordnade elementet `b` fungerar som innehållsområdet.
- Kopplar automatiskt taggen `a[value]`, `a` till `c-select` (baserat på gruppens värdeattribut och det valda alternativvärdet) och innehållsnoden `b[slot]`.
- Stöder automatiskt kvarhållande av det för närvarande aktiverade objektet till `localStorage` genom attributet `key`.
- Stöder automatisk lagring och återställning av historiska underval för varje grupp till `localStorage` med `key + '/' + tab`.
- Stöder specificering av den aktiva fliken genom attributet `value`.

## Använd demon

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
    <b slot="js">JavaScript är ett skriptspråk</b>
    <b slot="rs">Rust är ett programmeringsspråk på systemnivå</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS är ett Unix-baserat grafiskt operativsystem</b>
    <b slot="win">Windows är ett operativsystem utvecklat av Microsoft</b>
  </b>
</c-tab-select>
```

## egendom

- `key`: nyckelnamn som används för lokallagringsbeständighet
- `value`: för närvarande valt värde

## Anpassade händelser

- `change`: Utlöses när du byter flik, `e.value` är `value` värdet för det för närvarande aktiverade alternativet.