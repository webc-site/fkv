# Tab page na may drop-down na pagpipilian

- Sinusuportahan ang pag-embed ng adaptive width na drop-down selection box (`<c-select>`) sa label, na ipapakita lamang kapag ang kasalukuyang pahina ng tab ay na-activate.
- Structural specification: Ginagamit ng child element ang `nav > a` bilang label navigation, at ang direktang child element na `b` ang nagsisilbing content area.
- Awtomatikong iniuugnay ang `a[value]` tag, `a` sa `c-select` (batay sa value attribute ng pangkat at sa napiling value ng opsyon) at sa `b[slot]` content node.
- Sinusuportahan ang awtomatikong pagpapatuloy ng kasalukuyang naka-activate na item sa `localStorage` sa pamamagitan ng attribute na `key`.
- Sinusuportahan ang awtomatikong pag-save at pagpapanumbalik ng mga makasaysayang sub-selection para sa bawat pangkat sa `localStorage` gamit ang `key + '/' + tab`.
- Sinusuportahan ang pagtukoy sa aktibong tab sa pamamagitan ng attribute na `value`.

## Gamitin ang demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>wika</span>
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
    <b slot="js">Ang JavaScript ay isang scripting language</b>
    <b slot="rs">Ang Rust ay isang programming language sa antas ng system</b>
  </b>
  <b slot="os">
    <b slot="mac">Ang macOS ay isang graphical na operating system na nakabatay sa Unix</b>
    <b slot="win">Ang Windows ay isang operating system na binuo ng Microsoft</b>
  </b>
</c-tab-select>
```

## ari-arian

- `key`: key name na ginamit para sa localStorage persistence
- `value`: kasalukuyang napiling halaga

## Mga custom na kaganapan

- `change`: Na-trigger kapag lumilipat ng mga tab, ang `e.value` ay ang `value` na halaga ng kasalukuyang naka-activate na opsyon.