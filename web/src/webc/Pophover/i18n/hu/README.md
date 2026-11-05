# lebegő buborék prompt

A lebegtető eszköztipp információinak megjelenítése, amikor az egeret mozgatják, vagy az elemet aktiválják.

- Natív webkomponensen alapul, könnyű és nagy teljesítményű
- Támogatja az adaptív térbeli pozicionálást négy irányban (class="top/bottom/left/right")
- Fekete mattüveg textúra gradiens hátteret és fehér karaktereket használ, kitűnő fizikai élkontúr kiemeléssel.
- Beépített pozíció-adaptív számítás, automatikusan beállítja a felugró irányt a nézetablakban fennmaradó helynek megfelelően
- Használja az `slot="pop"` parancsot a lebegő prompt tartalmak megjelenítéséhez

## Használd a demót

```html
<!-- Irány automatikus kiszámítása (ajánlott, nem kell osztályt deklarálni) -->
<c-pophover>
  <button>automatikus</button>
  <div slot="pop">Ez egy buborékhegy</div>
</c-pophover>

<!-- rögzített irány -->
<c-pophover class="left">
  <button>Balra</button>
  <div slot="pop">Ez egy buborékhegy</div>
</c-pophover>

<c-pophover class="right">
  <button>jobbra</button>
  <div slot="pop">Ez egy buborékhegy</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Le</button>
  <div slot="pop">Ez egy buborékhegy</div>
</c-pophover>

<c-pophover class="top">
  <button>felsőbbrendű</button>
  <div slot="pop">Ez egy buborékhegy</div>
</c-pophover>
```

## CSS-változók

Az egyéni stílusokat a következő CSS-változók támogatják:

| változó neve | alapértelmezett érték | szemléltet |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | A buborék eszköztippje és az aktiváló elem közötti tér |
| `--pophover-margin` | `24px` | A nézetablak élének biztonsági határa az adaptív pozicionálási számítás során |
| `--pophover-bg-top` | `#18181ce0` | Buborék gradiens háttér felső színe |
| `--pophover-bg-mid` | `#121215e5` | A buborék bal és jobb oldalán lévő nyilak háttérszíne |
| `--pophover-bg-bottom` | `#0c0c0feb` | A buborék gradiens hátterének alsó és felső és alsó oldalsó nyilai színe |

## Ikon elemleírás stílusa (`.Ico`)

A tiszta ikon típusú lebegő prompt gombok (például "Szerkesztés", "Törlés" stb.) esetén egységes ikont és lebegtetési stílust alkalmazhat úgy, hogy hozzáadja az `.Ico` osztály nevét magához a külső tárolóhoz vagy összetevőhöz.

### Szerkezeti példa

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">szerkeszteni</div>
  </c-pophover>
</b>
```

Az alapvető elrendezés, a méret (18x18 képpont) és a lebegtetés színváltoztatása egységes lett a `.Ico > c-pophover > a`-ben. A fejlesztőknek csak külön kell megadniuk a megfelelő ikonfájlt, például:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```