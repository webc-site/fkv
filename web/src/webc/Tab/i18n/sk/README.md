# Prepínateľné karty

- Vybraný stav štítku je hladko integrovaný s oblasťou obsahu nižšie.
- Štrukturálne špecifikácie: Podradené prvky sú pevne stanovené tak, aby používali `nav > a` ako navigáciu menoviek a priamy podradený prvok `b` sa používa ako oblasť obsahu.
- Automaticky priradiť značku `a[value]` k uzlu obsahu `b[slot]`.
- Podporuje automatické zachovanie aktuálnej aktívnej stránky na `localStorage` prostredníctvom atribútu `key`.
- Podporuje špecifikovanie stránky aktivovanej karty pomocou atribútu `value` (ak nie je k dispozícii žiadna vyrovnávacia pamäť a `value`, predvolene sa vyberie značka s `class="A"` alebo prvá značka).
- Ľahká implementácia Light DOM, žiadna prekážka Shadow DOM, vysoký stupeň voľnosti pri prispôsobení štýlu.

## Použite demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">jablko</a>
    <a value="banana">banán</a>
    <a value="orange">mandarínka</a>
  </nav>
  <b slot="apple">Jablko je výživné ovocie</b>
  <b slot="banana">Banán je tropické ovocie</b>
  <b slot="orange">Pomaranče sú bohaté na vitamín C</b>
</c-tab>
```

## majetku

- `key`: názov kľúča používaný pre trvalosť localStorage
- `value`: aktuálne vybratá hodnota

## Vlastné udalosti

- `change`: Spustí sa pri prepnutí karty, `e.value` je hodnota `value` aktuálne aktivovanej značky.