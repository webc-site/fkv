# Perjungiami skirtukai

- Pasirinkta etiketės būsena yra sklandžiai integruota į toliau esančią turinio sritį.
- Struktūrinės specifikacijos: antriniai elementai nustatomi naudoti `nav > a` kaip etiketės naršymą, o tiesioginis antrinis elementas `b` naudojamas kaip turinio sritis.
- Automatiškai susiekite `a[value]` žymą su `b[slot]` turinio mazgu.
- Palaikomas automatinis dabartinio aktyvaus puslapio išsaugojimas `localStorage` naudojant atributą `key`.
- Palaikomas suaktyvinto skirtuko puslapio nurodymas naudojant atributą `value` (jei nėra talpyklos ir `value`, žyma su `class="A"` arba pirmoji žyma bus pasirinkta pagal numatytuosius nustatymus).
- Lengvas Lengvas DOM diegimas, jokių šešėlinių DOM kliūčių, didelė stiliaus pritaikymo laisvė.

## Naudokite demonstracinę versiją

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">obuolys</a>
    <a value="banana">bananas</a>
    <a value="orange">mandarinas</a>
  </nav>
  <b slot="apple">Obuolys yra maistingas vaisius</b>
  <b slot="banana">Bananas yra tropinis vaisius</b>
  <b slot="orange">Apelsinuose gausu vitamino C</b>
</c-tab>
```

## nuosavybė

- `key`: rakto pavadinimas, naudojamas vietinei saugyklos išlikimui
- `value`: šiuo metu pasirinkta vertė

## Individualūs įvykiai

- `change`: suaktyvinama, kai perjungiamas skirtukas, `e.value` yra šiuo metu suaktyvintos žymos `value` reikšmė.