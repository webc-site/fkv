# Tab page b'għażla drop-down

- Jappoġġja l-inkorporazzjoni tal-kaxxa tal-għażla drop-down tal-wisa 'adattiva (`<c-select>`) fit-tikketta, li tintwera biss meta l-paġna tat-tab attwali tkun attivata.
- Speċifikazzjoni strutturali: L-element tifel juża `nav > a` bħala n-navigazzjoni tat-tikketta, u l-element tifel dirett `b` iservi bħala ż-żona tal-kontenut.
- Jassoċja awtomatikament it-tikketta `a[value]`, `a` ma' `c-select` (ibbażat fuq l-attribut tal-valur tal-grupp u l-valur tal-għażla magħżula) u n-nodu tal-kontenut `b[slot]`.
- Jappoġġja l-persistenza awtomatika tal-oġġett attivat bħalissa għal `localStorage` permezz tal-attribut `key`.
- Jappoġġja l-iffrankar awtomatiku u r-restawr ta' sotto-selezzjonijiet storiċi għal kull grupp għal `localStorage` bl-użu ta' `key + '/' + tab`.
- Jappoġġja l-ispeċifikazzjoni tat-tab attiva permezz tal-attribut `value`.

## Uża d-demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>lingwa</span>
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
    <b slot="js">JavaScript huwa lingwa tal-kitba</b>
    <b slot="rs">Rust hija lingwa ta 'programmar fil-livell tas-sistemi</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS hija sistema operattiva grafika bbażata fuq Unix</b>
    <b slot="win">Windows hija sistema operattiva żviluppata minn Microsoft</b>
  </b>
</c-tab-select>
```

## proprjetà

- `key`: isem ewlieni użat għall-persistenza tal-LokalStorage
- `value`: valur magħżul bħalissa

## Avvenimenti tad-dwana

- `change`: Triggered meta taqleb it-tabs, `e.value` huwa l-valur `value` tal-għażla attivata bħalissa.