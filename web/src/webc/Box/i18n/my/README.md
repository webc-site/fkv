# ပေါ့ပ်အပ်အလွှာကို ဖန်တီးပြီး ပြသပါ။

- ပေါ့ပ်အပ်အလွှာကို ဖန်တီးပြီး ပြသရန် လုပ်ဆောင်နိုင်သော ခေါ်ဆိုမှု
- ပိတ်သည့်အခါ DOM မှ အစိတ်အပိုင်းများကို အလိုအလျောက် ဖယ်ရှားပါ။
- Esc သော့ဖျက်သိမ်းခြင်းအပြုအမူကို ကာကွယ်ပါ။

## ဒီမိုကိုသုံးပါ။

```html
<button>ပေါ်လာရန် နှိပ်ပါ။</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "ခေါင်းစဉ်";
  description.textContent = "စာသား";
  button.className = "Btn Main";
  button.textContent = "အိုကေ";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## မျက်နှာပြင်ဘောင်များ

### ပုံသေထုတ်ယူထားသော လုပ်ဆောင်ချက်များ

`Box()`

- **ပြန်တန်ဖိုး**- `HTMLDialogElement`၊ `Box`စတိုင်အတန်း၏ ဒိုင်ယာလော့ဂ်ဒြပ်စင်ကို ပေါင်းထည့်လိုက်ပါပြီ။

## စတိုင်အတန်း

### `.Box`

နေရာချထားခြင်း၊ ဗဟိုပြုခြင်းနှင့် နောက်ခံမျက်နှာဖုံး မှုန်ဝါးခြင်းကို သတ်မှတ်ရန် `dialog`ဒြပ်စင်နှင့် သက်ဆိုင်သည်။

### `.Lg`

စတိုင်အတန်းသည် အတွင်းအကြောင်းအရာ၊ နောက်ခံအရောင်၊ လုံးဝန်းသောထောင့်များ၊ အရိပ်နှင့် ဒေါင်လိုက် အပြင်အဆင်ကို သတ်မှတ်ခြင်းနှင့် အတွင်းပိုင်း `h3`နှင့် `p` ၏ အပြင်အဆင်ကို သတ်မှတ်ခြင်း။