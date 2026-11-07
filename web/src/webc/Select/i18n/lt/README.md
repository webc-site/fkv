# Išskleidžiamasis pasirinkimas, plotis prisitaiko prie esamos prekės

Savosios `select` plotis pagal numatytuosius nustatymus nustatomas pagal ilgiausią parinkties tekstą. Perjungiant trumpas parinktis lieka per daug baltos vietos, o tai turi įtakos išvaizdai.

Šis komponentas skaidriai uždengia vietinę `select` virš `b` žymos, todėl plotis gali tiksliai prisitaikyti prie šiuo metu pasirinkto teksto ilgio.

- Visuose terminaluose palaikykite savojo `select` išskleidžiamojo meniu stilių ir pritaikymo neįgaliesiems palaikymą
- Palaiko vietines interaktyvias operacijas, pvz., visą klaviatūros skirtukų naršymą ir krypties klavišų perjungimą.
- Pritaikykite plotį prie dabartinės parinkties, kad pašalintumėte perteklinį baltą tarpą

## Naudokite demonstracinę versiją

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```