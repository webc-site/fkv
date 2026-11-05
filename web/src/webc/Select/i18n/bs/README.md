# Padajući izbor, širina se prilagođava trenutnoj stavci

Širina izvornog `select` je prema zadanim postavkama određena najdužim tekstom opcije. Prilikom prebacivanja kratkih opcija, previše je bijelog prostora, što utiče na izgled.

Ova komponenta transparentno pokriva izvorni `select` iznad oznake `b`, omogućavajući da se širina precizno prilagodi dužini trenutno odabranog teksta.

- Održavajte izvorne `select` stilove padajućeg menija i podršku za pristupačnost na svim terminalima
- Podržava prirodne interaktivne operacije kao što je kompletna navigacija tabulatorom na tastaturi i prebacivanje tastera za smer.
- Prilagodite širinu trenutnoj opciji kako biste eliminirali višak bijelog prostora

## Koristite demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```