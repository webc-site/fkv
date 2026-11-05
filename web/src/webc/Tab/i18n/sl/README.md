# Preklopljivi zavihki

- Izbrano stanje nalepke je neopazno integrirano s spodnjim območjem vsebine.
- Strukturne specifikacije: podrejeni elementi so določeni tako, da uporabljajo `nav > a` kot navigacijo po oznakah, neposredni podrejeni element `b` pa se uporablja kot področje vsebine.
- Samodejno povežite oznako `a[value]` z vozliščem vsebine `b[slot]`.
- Podpira samodejno ohranjanje trenutne aktivne strani na `localStorage` prek atributa `key`.
- Podpira določanje aktivirane strani z zavihki prek atributa `value` (če ni predpomnilnika in `value`, bo privzeto izbrana oznaka z `class="A"` ali prva oznaka).
- Lahka izvedba Light DOM, brez ovir Shadow DOM, visoka stopnja svobode pri prilagajanju sloga.

## Uporabite demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">jabolko</a>
    <a value="banana">banana</a>
    <a value="orange">mandarina</a>
  </nav>
  <b slot="apple">Jabolko je hranljivo sadje</b>
  <b slot="banana">Banana je tropski sadež</b>
  <b slot="orange">Pomaranče so bogate z vitaminom C</b>
</c-tab>
```

## premoženje

- `key`: ime ključa, ki se uporablja za obstojnost localStorage
- `value`: trenutno izbrana vrednost

## Dogodki po meri

- `change`: sproži se ob preklopu zavihka, `e.value` je vrednost `value` trenutno aktivirane oznake.