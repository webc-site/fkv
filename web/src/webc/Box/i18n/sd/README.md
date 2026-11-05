# پاپ اپ پرت ٺاهيو ۽ ڊسپلي ڪريو

- پاپ اپ پرت ٺاهڻ ۽ ڊسپلي ڪرڻ لاءِ فنڪشنل ڪال
- خودڪار طور تي DOM مان عناصر کي هٽايو جڏهن بند ڪيو وڃي
- Esc اهم منسوخي رويي کي روڪيو

## ڊيمو استعمال ڪريو

```html
<button>پاپ اپ ڪرڻ لاءِ ڪلڪ ڪريو</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "عنوان"؛
  description.textContent = "ٽيڪسٽ"؛
  button.className = "Btn Main";
  button.textContent = "ٺيڪ"؛
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## انٽرفيس پيٽرول

### ڊفالٽ برآمد ٿيل افعال

`Box()`

- **واپسي قدر**: `HTMLDialogElement`، `Box` اسٽائل ڪلاس جو ڊائلاگ عنصر شامل ڪيو ويو آهي.

## انداز ڪلاس

### `.Box`

لاڳو ٿئي ٿو `dialog` عنصر سيٽ ڪرڻ لاءِ پوزيشن، سينٽرنگ، ۽ پس منظر ماسڪ بلر.

### `.Lg`

اندروني مواد تي لاڳو ڪيل هڪ اسلوب ڪلاس، پس منظر جي رنگ کي ترتيب ڏيڻ، گول ڪنڊن، ڇانو ۽ عمودي ترتيب، ۽ اندروني `h3` ۽ `p` جي ترتيب جي وضاحت ڪندي.