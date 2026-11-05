# Spustni izbor, širina se prilagodi trenutnemu elementu

Širina izvornega `select` je privzeto določena z najdaljšim besedilom možnosti. Pri preklopu kratkih možnosti je preveč belega prostora, kar vpliva na videz.

Ta komponenta pregledno pokriva izvorni `select` nad oznako `b`, kar omogoča, da se širina natančno prilagodi dolžini trenutno izbranega besedila.

- Ohranite izvorne `select` sloge spustnih menijev in podporo za dostopnost na vseh terminalih
- Podpira izvorne interaktivne operacije, kot je popolna navigacija z zavihki na tipkovnici in preklapljanje smernih tipk.
- Prilagodite širino trenutni možnosti, da odstranite odvečni prazen prostor

## Uporabite demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```