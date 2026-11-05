# Skirtuko puslapis su išskleidžiamuoju meniu

- Palaiko prisitaikančio pločio išskleidžiamojo pasirinkimo laukelio (`<c-select>`) įterpimą į etiketę, kuris bus rodomas tik suaktyvinus dabartinį skirtuko puslapį.
- Struktūrinė specifikacija: antrinis elementas naudoja `nav > a` kaip etiketės naršymą, o tiesioginis antrinis elementas `b` naudojamas kaip turinio sritis.
- Automatiškai susieja `a[value]` žymą, `a` su `c-select` (remiantis grupės vertės atributu ir pasirinkta parinkties reikšme) ir `b[slot]` turinio mazgu.
- Palaikomas automatinis šiuo metu suaktyvinto elemento išsaugojimas `localStorage` naudojant atributą `key`.
- Palaikomas automatinis kiekvienos grupės istorinių subrinkimų išsaugojimas ir atkūrimas į `localStorage` naudojant `key + '/' + tab`.
- Palaiko aktyvaus skirtuko nurodymą naudojant `value` atributą.

## Naudokite demonstracinę versiją

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>kalba</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistema</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript yra skriptų kalba</b>
    <b slot="rs">Rust yra sistemos lygio programavimo kalba</b>
  </b>
  <b slot="os">
    <b slot="mac">„macOS“ yra „Unix“ pagrindu sukurta grafinė operacinė sistema</b>
    <b slot="win">„Windows“ yra „Microsoft“ sukurta operacinė sistema</b>
  </b>
</c-tab-select>
```

## nuosavybė

- `key`: rakto pavadinimas, naudojamas vietinei saugyklos išlikimui
- `value`: šiuo metu pasirinkta vertė

## Individualūs įvykiai

- `change`: suaktyvinama perjungiant skirtukus, `e.value` yra šiuo metu suaktyvintos parinkties `value` reikšmė.