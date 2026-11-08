# Mga istilo ng palalimbagan na na-optimize para sa karanasan sa pagbabasa

- **Basic typesetting**: Iangkop sa mga pamagat, talata, bold, italics, strikethrough, inline code, mga larawan at mga linyang naghahati.
- **Link animation**: Ipakita ang underline na animation kapag nagho-hover.
- **Listahan ng proyekto**: Gumagamit ang mga hindi nakaayos na listahan ng mga bullet, gumagamit ang mga nakaayos na listahan ng mga bilugan na serial number sa background, at sumusuporta sa multi-level nesting.
- **Task List**: Nagbibigay ng istilo ng check box, sumusuporta sa dapat gawin at nakumpletong katayuan.
- **Mga Tip sa Babala**: Sinusuportahan ang limang uri ng mga kahon ng babala: Tandaan, Tip, Mahalaga, Babala, at Pansin.
- **Mga Talahanayan at Code**: Nagbibigay ng pag-highlight ng table row hover, at mga bloke ng code na may mga anino at gradient na background.

## Gamitin ang demo

```html
<div class="Md">
  <h1>pamagat</h1>
  <p>Teksto ng talata at <a href="#">link</a></p>
  <hr>
  <h2>Pangalawang antas ng pamagat</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Item ng Gawain</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Nilalaman ng tala</p>
  </blockquote>
</div>
```

## klase ng istilo

Inilapat ang mga istilo sa mga elemento ng HTML sa ilalim ng lalagyan ng `.Md`.

### Estilo ng kahon ng alerto

Idagdag ang sumusunod na pangalan ng klase sa `blockquote` element na may `.q` pangalan ng klase upang i-toggle ang alert box:

- `note`: Mga Tala
- `tip`: Tip
- `important`: Mahalaga
- `warning`: babala
- `caution`: Tandaan