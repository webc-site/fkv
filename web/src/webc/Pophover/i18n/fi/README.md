# kelluva kupla kehote

Näytä osoittimen työkaluvihjetiedot, kun hiirtä pidetään päällä tai elementti on aktivoitu.

- Perustuu alkuperäiseen Web-komponenttiin, kevyt ja tehokas
- Tukee mukautuvaa spatiaalista paikannusta neljään suuntaan (class="top/bottom/left/right")
- Siinä on käytetty mustaa himmeä lasikuviotaustaa ja valkoisia merkkejä, joissa on upeat fyysiset reunaviivat.
- Sisäänrakennettu sijainnin mukautuva laskenta, säätää automaattisesti ponnahdusikkunan suunnan kuvaportissa jäljellä olevan tilan mukaan
- Käytä `slot="pop"` näyttääksesi kelluvan kehotteen sisällön

## Käytä demoa

```html
<!-- Laske suunta automaattisesti (suositus, luokkaa ei tarvitse ilmoittaa) -->
<c-pophover>
  <button>automaattinen</button>
  <div slot="pop">Tämä on kuplan kärki</div>
</c-pophover>

<!-- kiinteä suunta -->
<c-pophover class="left">
  <button>Vasen</button>
  <div slot="pop">Tämä on kuplan kärki</div>
</c-pophover>

<c-pophover class="right">
  <button>oikein</button>
  <div slot="pop">Tämä on kuplan kärki</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Alas</button>
  <div slot="pop">Tämä on kuplan kärki</div>
</c-pophover>

<c-pophover class="top">
  <button>ylivoimainen</button>
  <div slot="pop">Tämä on kuplan kärki</div>
</c-pophover>
```

## CSS-muuttujat

Tuki tyylin mukauttamiseen seuraavien CSS-muuttujien avulla:

| muuttujan nimi | oletusarvo | havainnollistaa |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Väli ponnahdusikkunan ja laukaisevan elementin välillä |
| `--pophover-margin` | `24px` | Turvallinen marginaali näkymäportin reunoilla, kun mukautuva sijaintilaskenta on käytössä |
| `--pophover-bg-top` | `#18181ce0` | Bubble kaltevuus taustan yläväri |
| `--pophover-bg-mid` | `#121215e5` | Ponnahdusikkunan vasemman ja oikean puolen nuolen taustaväri |
| `--pophover-bg-bottom` | `#0c0c0feb` | Ponnahdusikkunan liukuvan taustavärin alaosa ja ylä- ja alapuolen nuolien väri |

## Kuvakkeen työkaluvihjeen tyyli (`.Ico`)

Pelkille kuvaketyyppisille kelluville kehotepainikkeille (kuten "Muokkaa", "Poista" jne.) voit käyttää yhtenäistä kuvaketta ja osoittimen tyyliä lisäämällä `.Ico`-luokan nimi itse ulompaan säilöön tai komponenttiin.

### Rakenneesimerkki

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">muokata</div>
  </c-pophover>
</b>
```

Perusasettelu, koko (18x18px) ja hover-värimuutokset on yhtenäistetty kohdassa `.Ico > c-pophover > a`. Kehittäjien tarvitsee vain määrittää vastaava kuvaketiedosto erikseen, kuten:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```