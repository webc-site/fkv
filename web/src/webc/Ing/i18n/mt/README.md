# Indikatur tal-istatus tat-tagħbija jew tal-ipproċessar

- Unifika l-cursor fl-istat ta 'stennija
- Juri animazzjoni tmewwiġ biex tindika l-istatus tal-ipproċessar
- Applika maskra ċċajpar fuq formola biex tevita sottomissjonijiet duplikati

## Uża d-demo

```html
<!-- indikatur tat-tagħbija indipendenti -->
<div class="Ing"></div>

<!-- Formola tat-tagħbija bil-maskra -->
<form class="Ing">
  <input type="text" placeholder="Kaxxa tad-dħul">
  <button type="submit">jissottometti</button>
</form>
```

## klassi tal-istil

### `.Ing`
- L-istatus tal-pointer huwa `wait`.
- Element psewdo `::before` jirrendi animazzjoni tmewwiġ.
- Meta jintuża fuq l-element `form`, il-psewdo-element `::after` jirrendi maskra ċċajpar 1px, li jiddiżattiva l-interazzjoni u l-għażla tat-test.