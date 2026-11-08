# Ang mga istilo sa typography na-optimize alang sa kasinatian sa pagbasa

- **Basic typesetting**: Ipahiangay sa mga titulo, paragraph, bold, italics, strikethrough, inline code, mga hulagway ug mga linya nga nagbahin.
- **Link animation**: Ipakita ang underline nga animation kung mag-hover.
- **Listahan sa proyekto**: Ang dili han-ay nga mga lista naggamit og mga bala, ang gi-order nga mga lista naggamit og rounded nga mga serial number sa background, ug nagsuporta sa multi-level nesting.
- **Listahan sa Buluhaton**: Naghatag ug estilo sa check box, nagsuporta sa buhaton ug nahuman nga kahimtang.
- **Mga Tip sa Pagpasidaan**: Nagsuporta sa lima ka matang sa mga kahon sa pasidaan: Pahinumdom, Tip, Importante, Pasidaan, ug Atensyon.
- **Tables ug Code**: Naghatag ug table row hover highlighting, ug code blocks nga adunay mga anino ug gradient nga background.

## Gamita ang demo

```html
<div class="Md">
  <h1>titulo</h1>
  <p>Teksto sa parapo ug <a href="#">link</a></p>
  <hr>
  <h2>Ikaduha nga lebel nga titulo</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Buluhaton nga Butang</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Tan-awa ang sulod</p>
  </blockquote>
</div>
```

## estilo nga klase

Ang mga estilo gipadapat sa HTML nga mga elemento ubos sa `.Md` nga sudlanan.

### Estilo sa alert box

Idugang ang mosunod nga ngalan sa klase sa `blockquote` elemento nga adunay `.q` ngalan sa klase aron i-toggle ang alert box:

- `note`: Mga nota
- `tip`: Tip
- `important`: Importante
- `warning`: pasidaan
- `caution`: Pahinumdom