# Dangosydd statws llwytho neu brosesu

- Uno'r cyrchwr i gyflwr aros
- Yn dangos animeiddiad crychdonni i ddangos statws prosesu
- Rhowch fwgwd aneglur ar ffurflen i atal cyflwyniadau dyblyg

## Defnyddiwch y demo

```html
<!-- dangosydd llwytho annibynnol -->
<div class="Ing"></div>

<!-- Llwytho'r ffurflen gyda mwgwd -->
<form class="Ing">
  <input type="text" placeholder="Blwch mewnbwn">
  <button type="submit">cyflwyno</button>
</form>
```

## dosbarth arddull

### `.Ing`
- Statws pwyntydd yw `wait`.
- Mae elfen ffug `::before` yn gwneud animeiddiad crychdonni.
- Pan gaiff ei ddefnyddio ar yr elfen `form`, mae'r ffug-elfen `::after` yn gwneud mwgwd aneglur 1px, gan analluogi rhyngweithio a dewis testun.