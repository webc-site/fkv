# Vaihdettavat välilehdet

- Tarran valittu tila integroituu saumattomasti alla olevaan sisältöalueeseen.
- Rakenteelliset tiedot: Alatason elementit on määritetty käyttämään `nav > a` tunnistenavigointina ja suoraa alielementtiä `b` käytetään sisältöalueena.
- Liitä tunniste `a[value]` automaattisesti `b[slot]` sisältösolmuun.
- Tukee nykyisen aktiivisen sivun automaattista säilyttämistä `localStorage` -määritteellä `key`.
- Tukee aktivoidun välilehtisivun määrittämistä `value`-attribuutin avulla (jos välimuistia ja `value` ei ole, tunniste, jossa on `class="A"` tai ensimmäinen tunniste, valitaan oletuksena).
- Kevyt Kevyt DOM-toteutus, ei Shadow DOM -esteitä, suuri vapaus tyylin mukauttamisessa.

## Käytä demoa

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">omena</a>
    <a value="banana">banaani</a>
    <a value="orange">mandariini</a>
  </nav>
  <b slot="apple">Omena on ravitseva hedelmä</b>
  <b slot="banana">Banaani on trooppinen hedelmä</b>
  <b slot="orange">Appelsiinit sisältävät runsaasti C-vitamiinia</b>
</c-tab>
```

## omaisuutta

- `key`: avaimen nimi, jota käytetään paikallisen tallennustilan pysyvyyttä varten
- `value`: tällä hetkellä valittu arvo

## Mukautetut tapahtumat

- `change`: Käynnistyy, kun välilehteä vaihdetaan, `e.value` on tällä hetkellä aktivoidun tunnisteen `value` arvo.