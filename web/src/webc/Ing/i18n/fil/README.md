# Tagapagpahiwatig ng katayuan sa pag-load o pagproseso

- Pagsamahin ang cursor sa status ng paghihintay
- Nagpapakita ng ripple animation upang ipahiwatig ang katayuan sa pagpoproseso
- Maglagay ng blur mask sa isang form para maiwasan ang mga duplicate na pagsusumite

## Gamitin ang demo

```html
<!-- independiyenteng tagapagpahiwatig ng paglo-load -->
<div class="Ing"></div>

<!-- Naglo-load ng form na may maskara -->
<form class="Ing">
  <input type="text" placeholder="Input box">
  <button type="submit">isumite</button>
</form>
```

## klase ng istilo

### `.Ing`
- Ang katayuan ng pointer ay `wait`.
- Ang pseudo element na `::before` ay nagbibigay ng ripple animation.
- Kapag ginamit sa elementong `form`, nagre-render ang pseudo-element na `::after` ng 1px blur mask, hindi pagpapagana ng pakikipag-ugnayan at pagpili ng text.