# Tudalen tab gyda dewisiad cwymplen

- Yn cefnogi gwreiddio'r cwymplen dewis lled addasol (`<c-select>`) yn y label, a fydd ond yn cael ei arddangos pan fydd y dudalen tab gyfredol wedi'i actifadu.
- Manyleb strwythurol: Mae'r elfen plentyn yn defnyddio `nav > a` fel llywio'r label, ac mae'r elfen plentyn uniongyrchol `b` yn gwasanaethu fel y maes cynnwys.
- Yn cysylltu'r tag `a[value]` yn awtomatig, `a` gyda `c-select` (yn seiliedig ar briodwedd gwerth y grŵp a'r gwerth opsiwn a ddewiswyd) a'r nod cynnwys `b[slot]`.
- Yn cefnogi parhau'n awtomatig yr eitem sydd wedi'i actifadu ar hyn o bryd i `localStorage` trwy'r briodwedd `key`.
- Yn cefnogi cadw ac adfer is-ddewisiadau hanesyddol yn awtomatig ar gyfer pob grŵp i `localStorage` gan ddefnyddio `key + '/' + tab`.
- Yn cefnogi pennu'r tab gweithredol trwy'r briodwedd `value`.

## Defnyddiwch y demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>iaith</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>system</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">Iaith sgriptio yw JavaScript</b>
    <b slot="rs">Mae Rust yn iaith raglennu ar lefel systemau</b>
  </b>
  <b slot="os">
    <b slot="mac">Mae macOS yn system weithredu graffigol sy'n seiliedig ar Unix</b>
    <b slot="win">Mae Windows yn system weithredu a ddatblygwyd gan Microsoft</b>
  </b>
</c-tab-select>
```

## eiddo

- `key`: enw allweddol a ddefnyddir ar gyfer dyfalbarhad storio lleol
- `value`: gwerth a ddewiswyd ar hyn o bryd

## Digwyddiadau personol

- `change`: Wedi'i sbarduno wrth newid tabiau, `e.value` yw gwerth `value` yr opsiwn a weithredir ar hyn o bryd.