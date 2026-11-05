# výzva k plovoucí bublině

Zobrazte informace o nápovědě při najetí myší nebo aktivaci prvku.

- Založeno na nativní webové komponentě, lehké a vysoce výkonné
- Podporuje adaptivní prostorové umístění ve čtyřech směrech (class="top/bottom/left/right")
- Používá pozadí s přechodem textury z černého matného skla a bílé znaky s vynikajícími fyzickými obrysovými zvýrazněními.
- Vestavěný adaptivní výpočet polohy automaticky upraví směr vyskakování podle zbývajícího prostoru ve výřezu
- Použijte `slot="pop"` k zobrazení plovoucího obsahu výzvy

## Použijte demo

```html
<!-- Automaticky vypočítat směr (doporučeno, není třeba deklarovat třídu) -->
<c-pophover>
  <button>automatický</button>
  <div slot="pop">Toto je špička bubliny</div>
</c-pophover>

<!-- pevný směr -->
<c-pophover class="left">
  <button>Vlevo</button>
  <div slot="pop">Toto je špička bubliny</div>
</c-pophover>

<c-pophover class="right">
  <button>právo</button>
  <div slot="pop">Toto je špička bubliny</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Dolů</button>
  <div slot="pop">Toto je špička bubliny</div>
</c-pophover>

<c-pophover class="top">
  <button>lepší</button>
  <div slot="pop">Toto je špička bubliny</div>
</c-pophover>
```

## CSS proměnné

Vlastní styly jsou podporovány prostřednictvím následujících proměnných CSS:

| název proměnné | výchozí hodnota | ilustrovat |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Prostor mezi bublinovou nápovědou a spouštěcím prvkem |
| `--pophover-margin` | `24px` | Bezpečnostní rezerva okraje výřezu během výpočtu adaptivní polohy |
| `--pophover-bg-top` | `#18181ce0` | Horní barva pozadí bublina s přechodem |
| `--pophover-bg-mid` | `#121215e5` | Barva pozadí šipek na levé a pravé straně bubliny |
| `--pophover-bg-bottom` | `#0c0c0feb` | Barva spodní a horní a spodní boční šipky pozadí bublinového přechodu |

## Styl popisku ikony (`.Ico`)

Pro tlačítka s plovoucí výzvou čistě typu ikony (jako je „Upravit“, „Odstranit“ atd.) můžete použít jednotný styl ikony a ukazatele myši přidáním názvu třídy `.Ico` k samotnému vnějšímu kontejneru nebo komponentě.

### Příklad struktury

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">upravit</div>
  </c-pophover>
</b>
```

Změny základního rozvržení, velikosti (18x18px) a barev při umístění kurzoru byly v `.Ico > c-pophover > a` sjednoceny. Vývojáři potřebují pouze specifikovat příslušný soubor ikony samostatně, například:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```