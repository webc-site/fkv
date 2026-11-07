# هڪ بند بٽڻ سان هڪ پاپ اپ پرت ٺاهيو ۽ ڏيکاريو

- پاپ اپ پرت ٺاهڻ ۽ ڊسپلي ڪرڻ لاءِ فنڪشنل ڪال
- تعمير ٿيل بند بٽڻ، بند ڪرڻ لاء ڪلڪ ڪريو ۽ عنصر کي ڊوم مان هٽايو
- دٻايو Esc ان پٽ باڪس کي ختم ڪرڻ يا پاپ اپ پرت کي بند ڪرڻ لاءِ

## ڊيمو استعمال ڪريو

```html
<button>پاپ اپ ڪرڻ لاءِ ڪلڪ ڪريو</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "ٽيڪسٽ"؛
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## انٽرفيس پيٽرول

ڊفالٽ ايڪسپورٽ فنڪشن واپس اچي ٿو `HTMLDialogElement`، ۽ `Box` ۽ `X` طرز جا ڪلاس ڊائلاگ عنصر ۾ شامل ڪيا ويا آهن.

## انداز ڪلاس

### `.Box.X`

`dialog` عنصر تي لاڳو، ڊائلاگ جو انداز بيان ڪري ٿو.

### `a.X`

`a` عنصر تي لاڳو، بند بٽڻ جو انداز بيان ڪري ٿو.