# پاپ اپ پرت بنائیں اور ڈسپلے کریں۔

- پاپ اپ پرت بنانے اور ڈسپلے کرنے کے لیے فنکشنل کال
- بند ہونے پر DOM سے عناصر کو خودکار طور پر ہٹا دیں۔
- Esc کلید منسوخی کے رویے کو روکیں۔

## ڈیمو استعمال کریں۔

```html
<button>پاپ اپ کرنے کے لیے کلک کریں۔</button>

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
  description.textContent = "متن"؛
  button.className = "Btn Main";
  button.textContent = "ٹھیک ہے"؛
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## انٹرفیس کے پیرامیٹرز

### ڈیفالٹ برآمد شدہ افعال

`Box()`

- **واپسی قدر**: `HTMLDialogElement`، `Box` اسٹائل کلاس کا ڈائیلاگ عنصر شامل کر دیا گیا ہے۔

## سٹائل کلاس

### `.Box`

پوزیشننگ، سینٹرنگ، اور بیک گراؤنڈ ماسک بلر سیٹ کرنے کے لیے `dialog` عنصر پر لاگو ہوتا ہے۔

### `.Lg`

اندرونی مواد پر ایک اسٹائل کلاس کا اطلاق ہوتا ہے، پس منظر کا رنگ، گول کونوں، سائے اور عمودی لے آؤٹ کو ترتیب دینا، اور اندرونی `h3` اور `p` کی ترتیب کی وضاحت کرنا۔