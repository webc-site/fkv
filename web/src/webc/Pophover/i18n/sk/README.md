# výzva plávajúca bublina

Zobrazte informácie o popise pri umiestnení kurzora myši, keď je kurzor myši alebo je aktivovaný prvok.

- Založené na natívnom webovom komponente, ľahké a vysokovýkonné
- Podporuje adaptívne priestorové umiestnenie v štyroch smeroch (class="top/bottom/left/right")
- Používa pozadie s prechodom textúry z čierneho matného skla a biele znaky s vynikajúcimi fyzickými obrysmi okrajov.
- Zabudovaný adaptívny výpočet polohy automaticky upraví smer vyskakovania podľa zostávajúceho priestoru vo výreze
- Na zobrazenie obsahu plávajúcej výzvy použite `slot="pop"`

## Použite demo

```html
<!-- Automaticky vypočítať smer (odporúča sa, nie je potrebné deklarovať triedu) -->
<c-pophover>
  <button>automatické</button>
  <div slot="pop">Toto je bublinková špička</div>
</c-pophover>

<!-- pevný smer -->
<c-pophover class="left">
  <button>Vľavo</button>
  <div slot="pop">Toto je bublinková špička</div>
</c-pophover>

<c-pophover class="right">
  <button>správne</button>
  <div slot="pop">Toto je bublinková špička</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Dole</button>
  <div slot="pop">Toto je bublinková špička</div>
</c-pophover>

<c-pophover class="top">
  <button>nadriadený</button>
  <div slot="pop">Toto je bublinková špička</div>
</c-pophover>
```

## CSS premenné

Vlastné štýly sú podporované prostredníctvom nasledujúcich premenných CSS:

| názov premennej | predvolená hodnota | ilustrovať |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Priestor medzi bublinovým popisom a spúšťacím prvkom |
| `--pophover-margin` | `24px` | Bezpečnostná rezerva okraja výrezu počas výpočtu adaptívneho polohovania |
| `--pophover-bg-top` | `#18181ce0` | Horná farba pozadia s bublinkovým prechodom |
| `--pophover-bg-mid` | `#121215e5` | Farba pozadia šípok na ľavej a pravej strane bubliny |
| `--pophover-bg-bottom` | `#0c0c0feb` | Farba spodnej a hornej a dolnej bočnej šípky pozadia bublinového gradientu |

## Štýl popisu ikony (`.Ico`)

Pre tlačidlá s plávajúcou výzvou čisto typu ikony (ako napríklad „Upraviť“, „Odstrániť“ atď.) môžete použiť jednotný štýl ikony a kurzora pridaním názvu triedy `.Ico` do samotného vonkajšieho kontajnera alebo komponentu.

### Príklad štruktúry

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">upraviť</div>
  </c-pophover>
</b>
```

Zmeny základného rozloženia, veľkosti (18 x 18 pixelov) a farby pri umiestnení kurzora myši boli zjednotené v `.Ico > c-pophover > a`. Vývojári musia iba samostatne špecifikovať príslušný súbor ikony, ako napríklad:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```