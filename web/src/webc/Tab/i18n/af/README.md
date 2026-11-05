# Skakelbare oortjies

- Die geselekteerde toestand van die etiket is naatloos geïntegreer met die inhoudarea hieronder.
- Strukturele spesifikasies: Die kinderelemente is vasgestel om `nav > a` as die etiketnavigasie te gebruik, en die direkte kinderelement `b` word as die inhoudsarea gebruik.
- Assosieer die `a[value]` merker outomaties met die `b[slot]` inhoudsknooppunt.
- Ondersteun die outomatiese volharding van die huidige aktiewe bladsy na `localStorage` deur die `key` kenmerk.
- Ondersteun die spesifiseer van die geaktiveerde bladbladsy deur die `value`-kenmerk (as daar geen kas en `value` is nie, sal die merker met `class="A"` of die eerste merker by verstek gekies word).
- Liggewig ligte DOM-implementering, geen Shadow DOM-obstruksie nie, hoë mate van vryheid in stylaanpassing.

## Gebruik die demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">appel</a>
    <a value="banana">piesang</a>
    <a value="orange">mandaryn</a>
  </nav>
  <b slot="apple">Appel is 'n voedsame vrug</b>
  <b slot="banana">Piesang is 'n tropiese vrug</b>
  <b slot="orange">Lemoene is ryk aan vitamien C</b>
</c-tab>
```

## eiendom

- `key`: sleutelnaam gebruik vir localStorage-volharding
- `value`: tans geselekteerde waarde

## Pasgemaakte geleenthede

- `change`: Geaktiveer wanneer die oortjie verander word, `e.value` is die `value` waarde van die tans geaktiveerde merker.