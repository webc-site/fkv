# Tab page nga adunay drop-down nga pagpili

- Nagsuporta sa pag-embed sa adaptive width nga drop-down nga kahon sa pagpili (`<c-select>`) sa label, nga ipakita lamang kung ang kasamtangan nga panid sa tab ma-aktibo.
- Structural specification: Ang child element naggamit `nav > a` as the label navigation, and the direct child element `b` nagsilbi isip content area.
- Awtomatikong gi-associate ang `a[value]` tag, `a` sa `c-select` (base sa value attribute sa grupo ug sa pinili nga option value) ug ang `b[slot]` content node.
- Awtomatikong nagsuporta sa pagpadayon sa kasamtangang gi-aktibo nga butang ngadto sa `localStorage` pinaagi sa `key` attribute.
- Nagsuporta sa awtomatik nga pagtipig ug pagpasig-uli sa mga sub-pagpili sa kasaysayan alang sa matag grupo ngadto sa `localStorage` gamit ang `key + '/' + tab`.
- Nagsuporta sa pagpiho sa aktibong tab pinaagi sa `value` attribute.

## Gamita ang demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>pinulongan</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistema</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">Ang JavaScript kay usa ka scripting language</b>
    <b slot="rs">Ang Rust usa ka sistema sa lebel sa programming language</b>
  </b>
  <b slot="os">
    <b slot="mac">Ang macOS kay Unix-based graphical operating system</b>
    <b slot="win">Ang Windows usa ka operating system nga gimugna sa Microsoft</b>
  </b>
</c-tab-select>
```

## kabtangan

- `key`: yawe nga ngalan nga gigamit alang sa pagpadayon sa localStorage
- `value`: karon pinili nga bili

## Pasadya nga mga panghitabo

- `change`: Na-trigger sa dihang nagbalhin-balhin og tab, `e.value` mao ang `value` bili sa kasamtangang gi-aktibo nga opsyon.