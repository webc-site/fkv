# poziv za plavajoči oblaček

Prikaži informacije o opisu orodja za lebdenje, ko se miška pomakne ali je element aktiviran.

- Temelji na izvorni spletni komponenti, lahek in visoko zmogljiv
- Podpira prilagodljivo prostorsko pozicioniranje v štirih smereh (class="top/bottom/left/right")
- Uporablja gradientno ozadje teksture črnega motnega stekla in bele znake z izvrstnimi poudarki fizičnih kontur robov.
- Vgrajeni prilagodljivi izračun položaja samodejno prilagodi smer pojavnega okna glede na preostali prostor v vidnem polju
- Uporabite `slot="pop"` za prikaz plavajoče vsebine poziva

## Uporabite demo

```html
<!-- Samodejno izračuna smer (priporočeno, razreda ni treba navesti) -->
<c-pophover>
  <button>samodejno</button>
  <div slot="pop">To je konica z mehurčki</div>
</c-pophover>

<!-- fiksna smer -->
<c-pophover class="left">
  <button>levo</button>
  <div slot="pop">To je konica z mehurčki</div>
</c-pophover>

<c-pophover class="right">
  <button>desno</button>
  <div slot="pop">To je konica z mehurčki</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Dol</button>
  <div slot="pop">To je konica z mehurčki</div>
</c-pophover>

<c-pophover class="top">
  <button>nadrejeni</button>
  <div slot="pop">To je konica z mehurčki</div>
</c-pophover>
```

## spremenljivke CSS

Slogi po meri so podprti prek naslednjih spremenljivk CSS:

| ime spremenljivke | privzeta vrednost | ilustrirati |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Prostor med konico orodja oblačka in sprožilnim elementom |
| `--pophover-margin` | `24px` | Varnostna meja roba vidnega polja med prilagodljivim izračunom pozicioniranja |
| `--pophover-bg-top` | `#18181ce0` | Zgornja barva ozadja mehurčkov |
| `--pophover-bg-mid` | `#121215e5` | Barva ozadja puščic na levi in ​​desni strani oblačka |
| `--pophover-bg-bottom` | `#0c0c0feb` | Barva spodnje ter zgornje in spodnje stranske puščice ozadja preliva mehurčkov |

## Slog opisa orodja ikone (`.Ico`)

Za lebdeče pozivne gumbe tipa čiste ikone (kot so »Uredi«, »Izbriši« itd.) lahko uporabite poenoteno ikono in slog lebdenja tako, da dodate ime razreda `.Ico` zunanjemu vsebniku ali sami komponenti.

### Primer strukture

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">uredi</div>
  </c-pophover>
</b>
```

Osnovna postavitev, velikost (18x18px) in spremembe barve lebdenja so bile poenotene v `.Ico > c-pophover > a`. Razvijalci morajo samo ločeno določiti ustrezno datoteko ikone, na primer:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```