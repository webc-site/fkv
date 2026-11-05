# Zgjedhja me drop-down, gjerësia përshtatet me artikullin aktual

Gjerësia e origjinalit `select` përcaktohet nga teksti më i gjatë i opsionit si parazgjedhje. Kur ndërroni opsionet e shkurtra, ka shumë hapësirë ​​të bardhë, gjë që ndikon në pamjen.

Ky komponent mbulon në mënyrë transparente origjinalin `select` mbi etiketën `b`, duke lejuar që gjerësia të përshtatet me saktësi me gjatësinë e tekstit të përzgjedhur aktualisht.

- Ruani stilet e menysë së lëshimit origjinal `select` dhe mbështetjen e aksesueshmërisë në të gjitha terminalet
- Mbështet operacione interaktive vendase, si p.sh. navigimi i plotë i tastierës me skeda dhe ndërrimi i tastit të drejtimit.
- Përshtatni gjerësinë me opsionin aktual për të eliminuar hapësirën e tepërt të bardhë

## Përdorni demonstrimin

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```