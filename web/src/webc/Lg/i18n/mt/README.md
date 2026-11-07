# Sfond tal-ħġieġ matt u effett ta 'enfasi tat-tarf

Bord semi-trasparenti u dell ta 'ġewwa joħolqu punti ewlenin tat-tarf ta' mikro-eżenzjoni.
Bħala d-dipendenza fuq l-istil bażiku tal-komponenti tal-buttuna u l-kaxxa tad-dħul.

## Uża d-demo

```html
<main class="Lg">
  <article>
    <h3>titolu</h3>
    <pre>Test tal-kontenut</pre>
  </article>
</main>
```

## klassi tal-istil

### `.Lg`
Stil bażiku tal-karta. Applika `backdrop-filter` ċajpir, `border` bordura mikro-intaljata, u `box-shadow` dell ta’ ġewwa. Applika l-filtru `svg/glass.svg#h` meta tgħaddi.

### `main.Lg`
Il-kontenitur tal-blokk prinċipali tal-paġna. Kantunieri fit-tond `24px` b'ikkuttunar li jirrispondu (`16px` jekk il-wisa' huwa inqas minn `800px`, `24px` inkella).

### `.Btn`, `.BtnC`, `.Input`
Isem tal-klassi tal-komponent bażiku. Wara li tintroduċi dan l-istil, awtomatikament tikseb tqassim flessibbli, relazzjonijiet ġerarkiċi u proprjetajiet bażiċi tal-ħġieġ matt.