# ڊيبگنگ HTML ۽ JS ڪوڊ آن لائن

- ڪوڊ جي هيٺان ڊيبگنگ بار ڏيو ۽ ڪوڊ پين ۽ JSFiddle ڊيبگنگ پورٽل مهيا ڪريو
- منتخب ٿيل ڊيبگنگ پليٽ فارم کي خودڪار طور تي ياد ڪريو
- متحرڪ طور تي HTML، CSS ۽ JS ڪوڊ ٺاهڻ لاءِ `.gen` وصف ذريعي انجيڪشن ڪال بيڪس کي سپورٽ ڪري ٿو
- `language-js` ڪلاس جي نالي سان پاڻمرادو ٻار جي عنصرن کي سڃاڻو ۽ JS موڊ ۾ هلايو

## ڊيمو استعمال ڪريو

```html
<c-htm-demo><pre><code class="language-html">
<h1>ڊيمو</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // واپسي [html، css، js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## انٽرفيس پيٽرول

### خاصيتون ۽ طريقا

- `.gen`: انجيل ڪال بڪ فنڪشن. ** هي ڪال بڪ ڊيبگ بار کي ظاهر ڪرڻ لاءِ انجيل ڪيو وڃي. **
  - پيرا ميٽر:
    - `type`: داخل ٿيل مواد جو قسم (`HTML = 1`، `JS = 2`).
    - `text`: ڪوڊ ٽيڪسٽ ٻار جي عنصر مان ڪڍيو ويو.
  - واپسي جي قيمت: `[htm, css, js]`