# بند بٹن کے ساتھ ایک پاپ اپ پرت بنائیں اور ڈسپلے کریں۔

- پاپ اپ پرت بنانے اور ڈسپلے کرنے کے لیے فنکشنل کال
- بلٹ ان کلوز بٹن، DOM سے عنصر کو بند کرنے اور ہٹانے کے لیے کلک کریں۔
- ان پٹ باکس کو ڈی فوکس کرنے یا پاپ اپ لیئر کو بند کرنے کے لیے Esc دبائیں۔

## ڈیمو استعمال کریں۔

```html
<button>پاپ اپ کرنے کے لیے کلک کریں۔</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "متن"؛
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## انٹرفیس کے پیرامیٹرز

ڈیفالٹ ایکسپورٹ فنکشن `HTMLDialogElement` واپس کرتا ہے، اور `Box` اور `X` اسٹائل کلاسز کو ڈائیلاگ عنصر میں شامل کر دیا گیا ہے۔

## سٹائل کلاس

### `.Box.X`

`dialog` عنصر پر لاگو ہوتا ہے، ڈائیلاگ کا انداز بتاتا ہے۔

### `a.X`

`a` عنصر پر لاگو ہوتا ہے، بند بٹن کی طرز کی وضاحت کرتا ہے۔