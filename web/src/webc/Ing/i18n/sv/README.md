# Indikator för laddnings- eller bearbetningsstatus

- Förena markören till vänteläge
- Visar krusningsanimering för att indikera bearbetningsstatus
- Applicera en oskärpa mask på ett formulär för att förhindra dubbletter av inlämningar

## Använd demon

```html
<!-- oberoende lastindikator -->
<div class="Ing"></div>

<!-- Laddar form med mask -->
<form class="Ing">
  <input type="text" placeholder="Inmatningsruta">
  <button type="submit">överlämna</button>
</form>
```

## stilklass

### `.Ing`
- Pekarstatus är `wait`.
- Pseudoelementet `::before` återger en krusningsanimation.
- När det används på `form`-elementet, återger pseudoelementet `::after` en 1px oskärpa, vilket inaktiverar interaktion och textval.