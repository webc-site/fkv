# Schakelbare tabbladen

- De geselecteerde status van het label is naadloos geïntegreerd met het onderstaande inhoudsgebied.
- Structurele specificaties: De onderliggende elementen zijn zo ingesteld dat ze `nav > a` gebruiken als labelnavigatie, en het directe onderliggende element `b` wordt gebruikt als het inhoudsgebied.
- Koppel de tag `a[value]` automatisch aan het contentknooppunt `b[slot]`.
- Ondersteunt het automatisch persistent maken van de huidige actieve pagina naar `localStorage` via het `key` attribuut.
- Ondersteunt het specificeren van de geactiveerde tabbladpagina via het `value` attribuut (als er geen cache is en `value`, wordt standaard de tag met `class="A"` of de eerste tag geselecteerd).
- Lichtgewicht Light DOM-implementatie, geen Shadow DOM-obstructie, hoge mate van vrijheid in stijlaanpassing.

## Gebruik de demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">appel</a>
    <a value="banana">banaan</a>
    <a value="orange">mandarijn</a>
  </nav>
  <b slot="apple">Appel is een voedzame vrucht</b>
  <b slot="banana">Banaan is een tropische vrucht</b>
  <b slot="orange">Sinaasappelen zijn rijk aan vitamine C</b>
</c-tab>
```

## eigendom

- `key`: sleutelnaam gebruikt voor localStorage-persistentie
- `value`: momenteel geselecteerde waarde

## Aangepaste evenementen

- `change`: Geactiveerd wanneer het tabblad wordt gewijzigd, `e.value` is de `value` waarde van de momenteel geactiveerde tag.