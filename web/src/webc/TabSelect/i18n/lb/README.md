# Tab Säit mat Dropdown Auswiel

- Ënnerstëtzt embedding adaptiv Breet Drop-Down Auswiel Këscht (`<c-select>`) am Label, déi nëmmen ugewisen gëtt wann déi aktuell Tab Säit aktivéiert ass.
- Strukturell Spezifizéierung: D'Kandelement benotzt `nav > a` als Labelnavigatioun, an dat direkt Kannerelement `b` déngt als Inhaltsberäich.
- Automatesch assoziéiert den `a[value]` Tag, `a` mat `c-select` (baséiert op de Wäertattribut vun der Grupp an dem gewielten Optiounswäert) an dem `b[slot]` Inhaltsknuet.
- Ënnerstëtzt automatesch de momentan aktivéierten Artikel op `localStorage` duerch den `key` Attribut.
- Ënnerstëtzt automatesch späicheren a Restauratioun vun historeschen Ënner-Selektiounen fir all Grupp op `localStorage` benotzt `key + '/' + tab`.
- Ënnerstëtzt d'Spezifikatioun vum aktive Tab duerch d'`value` Attribut.

## Benotzt d'Demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>Sprooch</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>System</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript ass eng Skriptsprooch</b>
    <b slot="rs">Rust ass eng Programméierungssprooch op Systemniveau</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS ass en Unix-baséiert grafeschen Betribssystem</b>
    <b slot="win">Windows ass e Betribssystem entwéckelt vu Microsoft</b>
  </b>
</c-tab-select>
```

## Propriétéit

- `key`: Schlësselnumm benotzt fir d'LokalStorage Persistenz
- `value`: aktuell gewielte Wäert

## Benotzerdefinéiert Evenementer

- `change`: Ausgeléist wann Dir vun Tabs wiesselt, ass `e.value` de `value` Wäert vun der aktuell aktivéierter Optioun.