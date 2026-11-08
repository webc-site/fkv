# virtuaalinen vierityspalkki

- **Kaksisuuntainen vieritys**: Tarjoaa pystysuoran (`c-vs`) ja vaakasuuntaisen (`c-hs`) vierityksen.
- **Kokovastaus**: Tarkkaile sisällön ja alueen koon muutoksia, päivitä vierityspalkin koko ja sijainti automaattisesti ja piilota automaattisesti, kun ylivuotoa ei ole.
- **Vuorovaikutteinen tuki**: Tukee pyörän vierittämistä, liukusäätimen vetämistä ja radan napsautusten asettamista.
- **Tila Piilota**: Näytä ja laajenna liukusäädintä vierittäessäsi tai vierittäessäsi ja piilota se automaattisesti, kun se on paikallaan.

## pystysuora vieritys

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## vaakasuuntainen vieritys

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## tyylisiä osia

Mukauta Shadow DOM:n sisäisten elementtien tyyliä `::part()`:n avulla:

- `::part(scroll)`: Vieritä sisältöaluetta.
- `::part(bar)`: vierityspalkin raita.
- `::part(si)`: Vierityspalkin liukusäädin.
- `::part(i)`: Sisäinen käärekerros.
- `::part(drag)`: Vierityspalkin raita vedettävässä tilassa.

## tyyliluokka

- `.drag`: Käytetään `body`-elementtiin, mikä osoittaa, että se on vedettävässä tilassa.

## Mukautetut ominaisuudet

- `--si-bg`: Liukusäätimen taustaväri.
- `--si-anim`: Liukusäädinanimaatio.
- `--cursorScrollhSvg`: hiiren osoitin, kun vaakasuuntaista vierityspalkkia pidetään.
- `--cursorScrollvSvg`: Hiiren osoitin, kun pystysuoraa vierityspalkkia pidetään.