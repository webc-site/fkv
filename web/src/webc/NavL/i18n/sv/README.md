# Växla automatiskt navigationsmeny och returknapp

- Slå automatiskt in underordnade element för att generera navigeringsmeny och returknappstrukturer
- Spåra automatiskt routinghistorik och beräkna föregående returväg
- Visa navigeringsmenyn när rutten stämmer överens med menyalternativet, annars visas bakåtknappen

## Använd demon

```html
<c-nav-l>
  <a href="/">förstasidan</a>
  <a href="/doc">dokumentera</a>
</c-nav-l>
```

## stilklass

- `B`: aktiverar tillståndet för returknappen. Visa bakåtknappen och dölj navigeringsmenyn. Växlar automatiskt inom komponenten baserat på routing.