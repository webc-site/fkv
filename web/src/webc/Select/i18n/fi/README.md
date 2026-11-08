# Valinta alasvetovalikosta, leveys mukautuu nykyiseen kohteeseen

Alkuperäisen `select` leveys määräytyy oletusarvoisesti pisimmän vaihtoehtotekstin mukaan. Lyhyitä vaihtoehtoja vaihdettaessa jää liikaa valkoista tilaa, mikä vaikuttaa ulkonäköön.

Tämä komponentti peittää läpinäkyvästi alkuperäisen `select` -tunnisteen `b` yläpuolella, jolloin leveys mukautuu tarkasti valitun tekstin pituuteen.

- Säilytä alkuperäiset `select`-pudotusvalikon tyylit ja esteettömyystuki kaikissa päätteissä
- Tukee alkuperäisiä interaktiivisia toimintoja, kuten täydellistä näppäimistön välilehtinavigointia ja suuntanäppäinten vaihtoa.
- Mukauta leveys nykyiseen vaihtoehtoon ylimääräisen valkoisen tilan poistamiseksi

## Käytä demoa

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```