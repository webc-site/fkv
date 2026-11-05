# Pagina cu filă cu selecție drop-down

- Acceptă încorporarea casetei de selecție drop-down pentru lățimea adaptivă (`<c-select>`) în etichetă, care va fi afișată numai atunci când pagina curentă a filei este activată.
- Specificație structurală: elementul secundar folosește `nav > a` ca navigare pentru etichete, iar elementul secundar direct `b` servește ca zonă de conținut.
- Asociază automat eticheta `a[value]`, `a` cu `c-select` (pe baza atributului de valoare al grupului și a valorii opțiunii selectate) și a nodului de conținut `b[slot]`.
- Acceptă menținerea automată a elementului activat în prezent la `localStorage` prin atributul `key`.
- Acceptă salvarea și restabilirea automată a sub-selecțiilor istorice pentru fiecare grup în `localStorage` folosind `key + '/' + tab`.
- Acceptă specificarea filei active prin atributul `value`.

## Utilizați demonstrația

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>limbă</span>
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
    <b slot="js">JavaScript este un limbaj de scripting</b>
    <b slot="rs">Rust este un limbaj de programare la nivel de sistem</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS este un sistem de operare grafic bazat pe Unix</b>
    <b slot="win">Windows este un sistem de operare dezvoltat de Microsoft</b>
  </b>
</c-tab-select>
```

## proprietate

- `key`: numele cheii folosit pentru persistența localStorage
- `value`: valoarea selectată în prezent

## Evenimente personalizate

- `change`: declanșat la comutarea între file, `e.value` este valoarea `value` a opțiunii activate în prezent.