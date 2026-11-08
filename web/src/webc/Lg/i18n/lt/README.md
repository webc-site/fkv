# Matinio stiklo fonas ir kraštų paryškinimo efektas

Pusiau permatomas apvadas ir vidinis šešėlis sukuria mikroreljefo kraštų paryškinimą.
Kaip pagrindinė mygtuko ir įvesties dėžutės komponentų stiliaus priklausomybė.

## Naudokite demonstracinę versiją

```html
<main class="Lg">
  <article>
    <h3>titulą</h3>
    <pre>Turinio tekstas</pre>
  </article>
</main>
```

## stiliaus klasė

### `.Lg`
Pagrindinis kortelės stilius. Taikykite `backdrop-filter` suliejimą, `border` mikroreljefinį kraštą ir `box-shadow` vidinį šešėlį. Taikyti `svg/glass.svg#h` filtrą užvedus pelės žymeklį.

### `main.Lg`
Pagrindinis puslapio bloko sudėtinis rodinys. Suapvalinti kampai `24px` su reaguojančiu užpildu (`16px` jei plotis mažesnis nei `800px`, `24px` kitu atveju).

### `.Btn`, `.BtnC`, `.Input`
Pagrindinio komponento klasės pavadinimas. Įvedę šį stilių, automatiškai gausite lankstų išdėstymą, hierarchinius ryšius ir pagrindines matinio stiklo savybes.