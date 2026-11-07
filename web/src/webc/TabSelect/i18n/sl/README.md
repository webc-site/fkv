# Zavihek s spustnim izborom

- Podpira vdelavo spustnega izbirnega polja s prilagodljivo širino (`<c-select>`) v oznako, ki bo prikazano le, ko je aktivirana trenutna stran zavihka.
- Strukturna specifikacija: podrejeni element uporablja `nav > a` kot navigacijo po oznakah, neposredni podrejeni element `b` pa služi kot področje vsebine.
- Samodejno poveže oznako `a[value]`, `a` z `c-select` (na podlagi atributa vrednosti skupine in vrednosti izbrane možnosti) in vozlišče vsebine `b[slot]`.
- Podpira samodejno ohranjanje trenutno aktiviranega elementa na `localStorage` prek atributa `key`.
- Podpira samodejno shranjevanje in obnavljanje zgodovinskih podizborov za vsako skupino v `localStorage` z uporabo `key + '/' + tab`.
- Podpira določanje aktivnega zavihka prek atributa `value`.

## Uporabite demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>jezik</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistem</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript je skriptni jezik</b>
    <b slot="rs">Rust je programski jezik na sistemski ravni</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS je grafični operacijski sistem, ki temelji na Unixu</b>
    <b slot="win">Windows je operacijski sistem, ki ga je razvil Microsoft</b>
  </b>
</c-tab-select>
```

## premoženje

- `key`: ime ključa, ki se uporablja za obstojnost localStorage
- `value`: trenutno izbrana vrednost

## Dogodki po meri

- `change`: sproži se pri preklopu zavihkov, `e.value` je vrednost `value` trenutno aktivirane možnosti.