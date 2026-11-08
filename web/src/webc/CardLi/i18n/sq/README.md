# Ngarko dhe shfaq listën e kartave në mënyrë asinkrone

- **Ngarkimi asinkron**: Merrni të dhëna përmes një funksioni asinkron
- **Afishimi i statusit**: Mbështet tre gjendje: ngarkim, të dhëna bosh dhe kartë përkthimi.
- **Shfaqja fleksibël**: Kartat janë të renditura në rreshta

## Përdorni demonstrimin

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titulli " + i, "Përshkrimi " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parametrat e ndërfaqes

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funksioni i ngarkimit asinkron të të dhënave. Formati i çdo nënçelës është `[emri, prezantimi, lidhja]`.

## klasë stili

- `.CardLi`: element i jashtëm
- `.Card`: element i kartës
- `.Ing`: Po ngarkohet elementi i animuar