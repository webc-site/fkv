# Automaticky přepne navigační nabídku a tlačítko návratu

- Automaticky zalamovat podřízené prvky a vytvářet struktury navigační nabídky a tlačítka návratu
- Automaticky sledovat historii směrování a vypočítat předchozí návratovou cestu
- Zobrazte nabídku navigace, když trasa odpovídá položce nabídky, jinak zobrazte tlačítko Zpět

## Použijte demo

```html
<c-nav-l>
  <a href="/">titulní strana</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## stylová třída

- `B`: aktivuje stav tlačítka návratu. Zobrazit tlačítko Zpět a skrýt navigační nabídku. Automaticky přepínané v rámci komponenty na základě směrování.