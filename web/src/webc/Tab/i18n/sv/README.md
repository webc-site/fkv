# Växlingsbara flikar

- Det valda tillståndet för etiketten är sömlöst integrerat med innehållsområdet nedan.
- Strukturella specifikationer: De underordnade elementen är fixerade att använda `nav > a` som etikettnavigering, och det direkta underordnade elementet `b` används som innehållsområde.
- Koppla automatiskt taggen `a[value]` till `b[slot]` innehållsnoden.
- Stöder att automatiskt bevara den aktuella aktiva sidan till `localStorage` genom attributet `key`.
- Stöder specificering av den aktiverade fliksidan genom attributet `value` (om det inte finns någon cache och `value` kommer taggen med `class="A"` eller den första taggen att väljas som standard).
- Lättvikts Lätt DOM-implementering, inget Shadow DOM-hinder, hög grad av frihet i stilanpassning.

## Använd demon

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">äpple</a>
    <a value="banana">banan</a>
    <a value="orange">mandarin</a>
  </nav>
  <b slot="apple">Äpple är en näringsrik frukt</b>
  <b slot="banana">Banan är en tropisk frukt</b>
  <b slot="orange">Apelsiner är rika på vitamin C</b>
</c-tab>
```

## egendom

- `key`: nyckelnamn som används för lokallagringsbeständighet
- `value`: för närvarande valt värde

## Anpassade händelser

- `change`: Utlöses när fliken byts, `e.value` är `value` värdet för den för närvarande aktiverade taggen.