# Padajući odabir, širina se prilagođava trenutnoj stavci

Širina izvornog `select` prema zadanim postavkama određena je najdužim tekstom opcije. Prilikom prebacivanja kratkih opcija ima previše bijelog prostora, što utječe na izgled.

Ova komponenta transparentno prekriva izvorni `select` iznad oznake `b`, dopuštajući širini da se točno prilagodi duljini trenutno odabranog teksta.

- Održavajte izvorne `select` stilove padajućih izbornika i podršku za pristupačnost na svim terminalima
- Podržava izvorne interaktivne operacije kao što je potpuna navigacija karticama tipkovnice i prebacivanje tipki za smjer.
- Prilagodite širinu trenutnoj opciji kako biste uklonili višak bijelog prostora

## Koristite demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```