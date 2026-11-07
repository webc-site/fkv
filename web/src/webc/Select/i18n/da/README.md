# Drop-down valg, bredde tilpasser sig den aktuelle vare

Bredden af ​​native `select` bestemmes som standard af den længste valgtekst. Når du skifter korte indstillinger, er der for meget hvidt mellemrum, hvilket påvirker udseendet.

Denne komponent dækker gennemsigtigt det oprindelige `select` over `b` tagget, hvilket gør det muligt for bredden at tilpasse sig nøjagtigt til længden af ​​den aktuelt valgte tekst.

- Oprethold native `select` dropdown-menustile og tilgængelighedsunderstøttelse på alle terminaler
- Understøtter native interaktive operationer såsom komplet tastatur Tab-navigation og retningstastskift.
- Tilpas bredden til den aktuelle indstilling for at eliminere overskydende hvidt mellemrum

## Brug demoen

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```