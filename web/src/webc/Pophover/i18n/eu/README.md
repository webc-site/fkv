# burbuila flotagarria gonbita

Sagua pasatzean edo elementua aktibatuta dagoenean bistaratu pasatzeko tresna-informazioa.

- Web osagai natiboan oinarritutakoa, arina eta errendimendu handikoa
- Kokaleku espaziala moldagarria onartzen du lau norabidetan (klase = "goian/behean/ezkerrean/eskuinean")
- Beira izoztu beltzaren ehundura gradientearen atzeko planoa eta karaktere zuriak erabiltzen ditu, ertz fisikoaren ingerada nabarmen bikainekin.
- Eraikitako posizioaren kalkulu moldagarria, automatikoki doitzen du pop-up norabidea bistaratzean geratzen den espazioaren arabera
- Erabili `slot="pop"` gonbidapen-eduki flotagarria bistaratzeko

## Erabili demoa

```html
<!-- Kalkulatu automatikoki norabidea (gomendatua, ez da klasea deklaratu beharrik) -->
<c-pophover>
  <button>automatikoa</button>
  <div slot="pop">Hau burbuila punta bat da</div>
</c-pophover>

<!-- norabide finkoa -->
<c-pophover class="left">
  <button>Ezkerra</button>
  <div slot="pop">Hau burbuila punta bat da</div>
</c-pophover>

<c-pophover class="right">
  <button>eskubidea</button>
  <div slot="pop">Hau burbuila punta bat da</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Behera</button>
  <div slot="pop">Hau burbuila punta bat da</div>
</c-pophover>

<c-pophover class="top">
  <button>goi-mailakoa</button>
  <div slot="pop">Hau burbuila punta bat da</div>
</c-pophover>
```

## CSS aldagaiak

Estilo pertsonalizatuak CSS aldagai hauen bidez onartzen dira:

| aldagaiaren izena | balio lehenetsia | ilustratu |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Burbuilaren argibidearen eta elementu eragilearen arteko tartea |
| `--pophover-margin` | `24px` | Ikuspegiaren ertzaren segurtasun-marjina kokapen egokitzailea kalkulatzean |
| `--pophover-bg-top` | `#18181ce0` | Burbuila gradientearen goiko kolorea |
| `--pophover-bg-mid` | `#121215e5` | Burbuilaren ezkerreko eta eskuineko alboetako gezien atzeko kolorea |
| `--pophover-bg-bottom` | `#0c0c0feb` | Burbuila gradientearen hondoaren beheko eta goiko eta beheko alboko gezien kolorea |

## Ikonoen argibide-estiloa (`.Ico`)

Ikono motako gonbidapen-botoi mugikor hutsetarako (adibidez, "Editatu", "Ezabatu", etab.), ikono bateratua eta pasatzeko estiloa aplika ditzakezu `.Ico` klasearen izena kanpoko edukiontziari edo osagaiari berari gehituta.

### Egituraren adibidea

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">editatu</div>
  </c-pophover>
</b>
```

Oinarrizko diseinua, tamaina (18x18px) eta pasatzeko kolore aldaketak bateratu dira `.Ico > c-pophover > a`n. Garatzaileek dagokion ikono-fitxategia bereizita bakarrik zehaztu behar dute, hala nola:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```