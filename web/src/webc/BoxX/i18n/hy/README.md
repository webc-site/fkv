# Ստեղծեք և ցուցադրեք թռուցիկ շերտ փակման կոճակով

- Ֆունկցիոնալ զանգ՝ թռուցիկ շերտ ստեղծելու և ցուցադրելու համար
- Ներկառուցված փակ կոճակ, սեղմեք փակելու և տարրը DOM-ից հեռացնելու համար
- Սեղմեք Esc՝ մուտքագրման տուփը ապակենտրոնացնելու կամ թռուցիկ շերտը փակելու համար

## Օգտագործեք ցուցադրությունը

```html
<button>Սեղմեք՝ բաց թողնելու համար</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = «տեքստ»;
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Ինտերֆեյսի պարամետրեր

Արտահանման կանխադրված գործառույթը վերադարձնում է `HTMLDialogElement`, իսկ `Box` և `X` ոճի դասերը ավելացվել են երկխոսության տարրին:

## ոճի դաս

### `.Box.X`

Կիրառվելով `dialog` տարրի վրա, սահմանում է երկխոսության ոճը:

### `a.X`

Կիրառվելով `a` տարրի վրա, սահմանում է փակման կոճակի ոճը: