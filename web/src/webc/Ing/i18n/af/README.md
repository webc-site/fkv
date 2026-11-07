# Aanwyser van laai of verwerking status

- Verenig die wyser in wagtoestand
- Toon rimpel-animasie om verwerkingstatus aan te dui
- Pas 'n vervaagmasker op 'n vorm om duplikaatvoorleggings te voorkom

## Gebruik die demo

```html
<!-- onafhanklike laai aanwyser -->
<div class="Ing"></div>

<!-- Laai vorm met masker -->
<form class="Ing">
  <input type="text" placeholder="Invoerkassie">
  <button type="submit">indien</button>
</form>
```

## styl klas

### `.Ing`
- Wyserstatus is `wait`.
- Pseudo-element `::before` lewer 'n rimpel-animasie.
- Wanneer dit op die `form`-element gebruik word, lewer die pseudo-element `::after` 'n 1px-vervaagmasker, wat interaksie en tekskeuse deaktiveer.