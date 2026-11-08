# Vervolgkeuzelijst, breedte past zich aan het huidige item aan

De breedte van native `select` wordt standaard bepaald door de langste optietekst. Bij het schakelen tussen korte opties is er te veel witruimte, wat de weergave beïnvloedt.

Deze component bedekt transparant de native `select` boven de `b` tag, waardoor de breedte zich nauwkeurig kan aanpassen aan de lengte van de momenteel geselecteerde tekst.

- Behoud native `select` vervolgkeuzemenustijlen en toegankelijkheidsondersteuning op alle terminals
- Ondersteunt native interactieve bewerkingen zoals volledige toetsenbordtabnavigatie en schakelen tussen richtingstoetsen.
- Pas de breedte aan de huidige optie aan om overtollige witruimte te elimineren

## Gebruik de demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```