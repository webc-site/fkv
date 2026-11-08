# Välilehti, jossa on avattava valinta

- Tukee mukautuvan leveyden avattavan valintaruudun (`<c-select>`) upottamista tarraan, joka näkyy vain, kun nykyinen välilehtisivu on aktivoitu.
- Rakennemääritys: Alatason elementti käyttää `nav > a`-elementtiä tunnisteen navigointina ja suora alielementti `b` toimii sisältöalueena.
- Liittää automaattisesti tunnisteen `a[value]` `a` -tunnisteen `c-select` (ryhmän arvoattribuutin ja valitun vaihtoehdon arvon perusteella) ja `b[slot]` -sisältösolmuun.
- Tukee tällä hetkellä aktivoidun kohteen automaattista säilyttämistä `localStorage` -määritteellä `key`.
- Tukee kunkin ryhmän historiallisten alivalintojen automaattista tallennusta ja palauttamista kohtaan `localStorage` käyttämällä `key + '/' + tab`.
- Tukee aktiivisen välilehden määrittämistä `value`-attribuutin avulla.

## Käytä demoa

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>kieli</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>järjestelmä</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript on skriptikieli</b>
    <b slot="rs">Rust on järjestelmätason ohjelmointikieli</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS on Unix-pohjainen graafinen käyttöjärjestelmä</b>
    <b slot="win">Windows on Microsoftin kehittämä käyttöjärjestelmä</b>
  </b>
</c-tab-select>
```

## omaisuutta

- `key`: avaimen nimi, jota käytetään paikallisen tallennustilan pysyvyyttä varten
- `value`: tällä hetkellä valittu arvo

## Mukautetut tapahtumat

- `change`: laukeaa välilehtiä vaihdettaessa, `e.value` on tällä hetkellä käytössä olevan vaihtoehdon `value` arvo.