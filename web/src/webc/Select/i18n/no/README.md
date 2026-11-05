# Nedtrekksvalg, bredde tilpasser seg gjeldende vare

Bredden på native `select` bestemmes av den lengste alternativteksten som standard. Når du bytter korte alternativer, er det for mye mellomrom, noe som påvirker utseendet.

Denne komponenten dekker transparent den opprinnelige `select` over `b`-taggen, slik at bredden kan tilpasses nøyaktig til lengden på den valgte teksten.

- Oppretthold innebygde `select` rullegardinmenystiler og tilgjengelighetsstøtte på alle terminaler
- Støtter native interaktive operasjoner som komplett tastatur Tabnavigering og retningstastbytte.
- Tilpass bredden til det gjeldende alternativet for å eliminere overflødig mellomrom

## Bruk demoen

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```