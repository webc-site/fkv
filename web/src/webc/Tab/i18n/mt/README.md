# Tabs jaqilbu

- L-istat magħżul tat-tikketta huwa integrat bla xkiel mal-qasam tal-kontenut hawn taħt.
- Speċifikazzjonijiet strutturali: L-elementi tfal huma ffissati biex jużaw `nav > a` bħala n-navigazzjoni tat-tikketta, u l-element tifel dirett `b` jintuża bħala ż-żona tal-kontenut.
- Assoċja awtomatikament it-tikketta `a[value]` man-nodu tal-kontenut `b[slot]`.
- Jappoġġja l-persisti awtomatika tal-paġna attiva attwali għal `localStorage` permezz tal-attribut `key`.
- Jappoġġja l-ispeċifikazzjoni tal-paġna tat-tab attivata permezz tal-attribut `value` (jekk ma jkunx hemm cache u `value`, it-tikketta b'`class="A"` jew l-ewwel tikketta tintgħażel b'mod awtomatiku).
- Implimentazzjoni ħafifa tad-DOM Light, l-ebda xkiel Shadow DOM, grad għoli ta 'libertà fl-adattament tal-istil.

## Uża d-demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">tuffieħ</a>
    <a value="banana">banana</a>
    <a value="orange">mandarina</a>
  </nav>
  <b slot="apple">It-tuffieħ huwa frott nutrittiv</b>
  <b slot="banana">Il-banana hija frotta tropikali</b>
  <b slot="orange">Il-larinġ huwa rikk fil-vitamina Ċ</b>
</c-tab>
```

## proprjetà

- `key`: isem ewlieni użat għall-persistenza tal-LokalStorage
- `value`: valur magħżul bħalissa

## Avvenimenti tad-dwana

- `change`: Triggered meta t-tab tinbidel, `e.value` huwa l-valur `value` tat-tikketta attivata bħalissa.