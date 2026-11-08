# Kartica s odabirom s padajućeg izbornika

- Podržava ugrađivanje padajućeg okvira za odabir prilagodljive širine (`<c-select>`) u oznaku, koji će biti prikazan samo kada je trenutna stranica kartice aktivirana.
- Strukturna specifikacija: podređeni element koristi `nav > a` kao navigaciju oznake, a izravni podređeni element `b` služi kao područje sadržaja.
- Automatski pridružuje oznaku `a[value]`, `a` s `c-select` (na temelju atributa vrijednosti grupe i vrijednosti odabrane opcije) i čvor sadržaja `b[slot]`.
- Podržava automatsko zadržavanje trenutno aktivirane stavke na `localStorage` putem atributa `key`.
- Podržava automatsko spremanje i vraćanje povijesnih pod-odabira za svaku grupu u `localStorage` koristeći `key + '/' + tab`.
- Podržava navođenje aktivne kartice putem atributa `value`.

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
      <span>sustav</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript je skriptni jezik</b>
    <b slot="rs">Rust je programski jezik na razini sustava</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS je grafički operativni sustav temeljen na Unixu</b>
    <b slot="win">Windows je operativni sustav koji je razvio Microsoft</b>
  </b>
</c-tab-select>
```

## imovine

- `key`: naziv ključa koji se koristi za postojanost localStorage
- `value`: trenutno odabrana vrijednost

## Prilagođeni događaji

- `change`: Pokreće se prilikom prebacivanja kartica, `e.value` je `value` vrijednost trenutno aktivirane opcije.