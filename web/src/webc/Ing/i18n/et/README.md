# Laadimise või töötlemise oleku indikaator

- Ühendage kursor ooteolekusse
- Kuvab töötlemise oleku näitamiseks pulsatsioonianimatsiooni
- Dubleerivate esitamiste vältimiseks rakendage vormile hägususmask

## Kasutage demo

```html
<!-- sõltumatu laadimisnäidik -->
<div class="Ing"></div>

<!-- Vormi laadimine maskiga -->
<form class="Ing">
  <input type="text" placeholder="Sisestuskast">
  <button type="submit">esitama</button>
</form>
```

## stiiliklass

### `.Ing`
- Kursori olek on `wait`.
- Pseudoelement `::before` renderdab pulsatsioonianimatsiooni.
- Kui seda kasutatakse elemendil `form`, renderdab pseudoelement `::after` 1 piksli suuruse hägususmaski, mis keelab interaktsiooni ja tekstivaliku.