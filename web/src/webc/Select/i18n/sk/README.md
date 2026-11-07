# Rozbaľovací výber, šírka sa prispôsobí aktuálnej položke

Šírka natívneho `select` je predvolene určená najdlhším textom možnosti. Pri prepínaní krátkych možností je príliš veľa bieleho priestoru, čo ovplyvňuje vzhľad.

Tento komponent transparentne pokrýva natívnu značku `select` nad značkou `b`, vďaka čomu sa šírka presne prispôsobí dĺžke aktuálne vybratého textu.

- Udržujte natívne `select` štýly rozbaľovacej ponuky a podporu dostupnosti na všetkých termináloch
- Podporuje natívne interaktívne operácie, ako je úplná navigácia pomocou klávesnice a prepínanie smerových tlačidiel.
- Prispôsobte šírku aktuálnej možnosti, aby ste odstránili prebytočný biely priestor

## Použite demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```