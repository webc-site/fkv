# Bladsy met aftreklys

- Ondersteun die inbedding van aanpasbare breedte-aftrekkieskiesboks (`<c-select>`) in die etiket, wat slegs vertoon sal word wanneer die huidige oortjiebladsy geaktiveer is.
- Strukturele spesifikasie: Die kinderelement gebruik `nav > a` as die etiketnavigasie, en die direkte kinderelement `b` dien as die inhoudsarea.
- Assosieer outomaties die `a[value]` merker, `a` met `c-select` (gebaseer op die groep se waarde-kenmerk en die geselekteerde opsiewaarde) en die `b[slot]` inhoudsknooppunt.
- Ondersteun die outomatiese volharding van die tans geaktiveerde item na `localStorage` deur die `key` kenmerk.
- Ondersteun outomatiese stoor en herstel van historiese sub-keuses vir elke groep na `localStorage` deur `key + '/' + tab` te gebruik.
- Ondersteun die spesifiseer van die aktiewe oortjie deur die `value` kenmerk.

## Gebruik die demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>taal</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>stelsel</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript is 'n skriftaal</b>
    <b slot="rs">Rust is 'n stelselvlak-programmeertaal</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS is 'n Unix-gebaseerde grafiese bedryfstelsel</b>
    <b slot="win">Windows is 'n bedryfstelsel wat deur Microsoft ontwikkel is</b>
  </b>
</c-tab-select>
```

## eiendom

- `key`: sleutelnaam gebruik vir localStorage-volharding
- `value`: tans geselekteerde waarde

## Pasgemaakte geleenthede

- `change`: Geaktiveer wanneer oortjies verander word, `e.value` is die `value` waarde van die tans geaktiveerde opsie.