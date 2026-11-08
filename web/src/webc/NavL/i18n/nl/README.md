# Automatisch schakelen tussen navigatiemenu en terugkeerknop

- Verpak onderliggende elementen automatisch om navigatiemenu's en terugkeerknopstructuren te genereren
- Volg automatisch de routegeschiedenis en bereken het vorige retourpad
- Geef het navigatiemenu weer als de route overeenkomt met het menu-item, geef anders de terugknop weer

## Gebruik de demo

```html
<c-nav-l>
  <a href="/">voorpagina</a>
  <a href="/doc">document</a>
</c-nav-l>
```

## stijl klasse

- `B`: activeert de status van de retourknop. Toon de terugknop en verberg het navigatiemenu. Automatisch geschakeld binnen het onderdeel op basis van routing.