# Nolaižamā izvēle, platums pielāgojas pašreizējam vienumam

Vietējās `select` platums pēc noklusējuma tiek noteikts pēc garākā opcijas teksta. Pārslēdzot īsās opcijas, ir pārāk daudz baltās vietas, kas ietekmē izskatu.

Šis komponents caurspīdīgi pārklāj vietējo `select` virs `b` taga, ļaujot platumam precīzi pielāgoties pašlaik atlasītā teksta garumam.

- Saglabājiet vietējos `select` nolaižamo izvēlņu stilus un pieejamības atbalstu visos termināļos
- Atbalsta vietējās interaktīvās darbības, piemēram, pilnīgu tastatūras cilnes navigāciju un virziena taustiņu pārslēgšanu.
- Pielāgojiet platumu pašreizējai opcijai, lai novērstu lieko balto atstarpi

## Izmantojiet demonstrāciju

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```