# Přepínatelné záložky

- Vybraný stav štítku je hladce integrován s oblastí obsahu níže.
- Strukturální specifikace: Podřízené prvky jsou pevně nastaveny tak, aby jako navigaci štítků používaly `nav > a` a jako oblast obsahu se používá přímý podřízený prvek `b`.
- Automaticky přiřadit značku `a[value]` k uzlu obsahu `b[slot]`.
- Podporuje automatické zachování aktuální aktivní stránky na `localStorage` prostřednictvím atributu `key`.
- Podporuje specifikaci stránky aktivované karty pomocí atributu `value` (pokud není mezipaměť a `value`, bude ve výchozím nastavení vybrána značka s `class="A"` nebo první značka).
- Lehká implementace Light DOM, žádné překážky Shadow DOM, vysoký stupeň svobody v přizpůsobení stylu.

## Použijte demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">jablko</a>
    <a value="banana">banán</a>
    <a value="orange">mandarinka</a>
  </nav>
  <b slot="apple">Jablko je výživné ovoce</b>
  <b slot="banana">Banán je tropické ovoce</b>
  <b slot="orange">Pomeranče jsou bohaté na vitamín C</b>
</c-tab>
```

## vlastnictví

- `key`: název klíče používaný pro persistenci localStorage
- `value`: aktuálně vybraná hodnota

## Vlastní akce

- `change`: Spustí se při přepnutí karty, `e.value` je hodnota `value` aktuálně aktivované značky.