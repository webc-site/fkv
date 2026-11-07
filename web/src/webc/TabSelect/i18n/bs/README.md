# Kartica sa padajućim izborom

- Podržava ugrađivanje padajućeg okvira za odabir prilagodljive širine (`<c-select>`) u naljepnicu, koji će biti prikazan samo kada je trenutna stranica kartice aktivirana.
- Strukturna specifikacija: podređeni element koristi `nav > a` kao navigaciju oznakom, a direktni podređeni element `b` služi kao područje sadržaja.
- Automatski povezuje oznaku `a[value]`, `a` sa `c-select` (zasnovano na atributu vrijednosti grupe i vrijednosti odabrane opcije) i sadržajnom čvoru `b[slot]`.
- Podržava automatsko zadržavanje trenutno aktivirane stavke na `localStorage` kroz atribut `key`.
- Podržava automatsko spremanje i vraćanje historijskih podizbora za svaku grupu na `localStorage` koristeći `key + '/' + tab`.
- Podržava određivanje aktivne kartice preko `value` atributa.

## Koristite demo

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
    <b slot="rs">Rust je programski jezik na nivou sistema</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS je grafički operativni sistem baziran na Unixu</b>
    <b slot="win">Windows je operativni sistem koji je razvio Microsoft</b>
  </b>
</c-tab-select>
```

## imovine

- `key`: ime ključa koji se koristi za postojanost localStorage
- `value`: trenutno odabrana vrijednost

## Prilagođeni događaji

- `change`: Pokreće se prilikom promjene kartica, `e.value` je `value` vrijednost trenutno aktivirane opcije.