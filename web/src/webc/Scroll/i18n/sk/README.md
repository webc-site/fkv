# virtuálny posúvač

- **Obojsmerné posúvanie**: Poskytuje vertikálne (`c-vs`) a horizontálne (`c-hs`) posúvanie.
- **Odozva na veľkosť**: Sledujte zmeny obsahu a veľkosti oblasti, automaticky aktualizujte veľkosť a polohu posúvača a automaticky sa skryte, keď nedochádza k pretečeniu.
- **Interaktívna podpora**: Podporuje rolovanie kolieskom, posúvanie posúvača a umiestnenie kliknutia na stopu.
- **Skryť stav**: Zobrazte a rozšírte posúvač pri umiestnení kurzora myši alebo posúvaní a automaticky ho skryte, keď stojíte.

## vertikálne posúvanie

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## horizontálne rolovanie

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## štýlové časti

Prispôsobte štýl vnútorných prvkov Shadow DOM pomocou `::part()`:

- `::part(scroll)`: Posúvajte oblasť obsahu.
- `::part(bar)`: dráha posúvača.
- `::part(si)`: Posúvač posúvača.
- `::part(i)`: Vnútorná obalová vrstva.
- `::part(drag)`: Dráha posúvača v stave ťahania.

## štýlová trieda

- `.drag`: Použije sa na prvok `body`, čo znamená, že je v stave posúvania myšou.

## Vlastné vlastnosti

- `--si-bg`: Farba pozadia posúvača.
- `--si-anim`: Animácia posúvača.
- `--cursorScrollhSvg`: Kurzor myši pri umiestnení vodorovného posúvača.
- `--cursorScrollvSvg`: Kurzor myši pri umiestnení zvislého posúvača.