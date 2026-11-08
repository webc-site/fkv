# Indicator van laad- of verwerkingsstatus

- Verenig de cursor in de wachtstatus
- Toont rimpelanimatie om de verwerkingsstatus aan te geven
- Pas een vervagingsmasker toe op een formulier om dubbele indieningen te voorkomen

## Gebruik de demo

```html
<!-- onafhankelijke laadindicator -->
<div class="Ing"></div>

<!-- Formulier laden met masker -->
<form class="Ing">
  <input type="text" placeholder="Invoervak">
  <button type="submit">indienen</button>
</form>
```

## stijl klasse

### `.Ing`
- De status van de aanwijzer is `wait`.
- Pseudo-element `::before` geeft een rimpelanimatie weer.
- Bij gebruik op het `form` element geeft het pseudo-element `::after` een vervagingsmasker van 1px weer, waardoor interactie en tekstselectie worden uitgeschakeld.