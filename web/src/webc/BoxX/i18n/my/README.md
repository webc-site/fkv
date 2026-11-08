# ပိတ်ခလုတ်ဖြင့် ပေါ့ပ်အပ်အလွှာကို ဖန်တီးပြီး ပြသပါ။

- ပေါ့ပ်အပ်အလွှာကို ဖန်တီးပြီး ပြသရန် လုပ်ဆောင်နိုင်သော ခေါ်ဆိုမှု
- Built-in အပိတ်ခလုတ်၊ ပိတ်ရန် နှိပ်ပြီး DOM မှ အစိတ်အပိုင်းကို ဖယ်ရှားပါ။
- အဝင်အကွက်ကို အာရုံစိုက်ရန် သို့မဟုတ် ပေါ့ပ်အပ်အလွှာကို ပိတ်ရန် Esc ကို နှိပ်ပါ။

## ဒီမိုကိုသုံးပါ။

```html
<button>ပေါ်လာရန် နှိပ်ပါ။</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "စာသား";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## မျက်နှာပြင်ဘောင်များ

မူရင်းထုတ်ယူမှုလုပ်ဆောင်ချက်သည် `HTMLDialogElement`နှင့် `Box`နှင့် `X`စတိုင်အတန်းများကို ဒိုင်ယာလော့ဂ်ဒြပ်စင်သို့ ပေါင်းထည့်ထားသည်။

## စတိုင်အတန်း

### `.Box.X`

`dialog`ဒြပ်စင်သို့ အသုံးချပြီး ဒိုင်ယာလော့ဂ်စတိုင်ကို သတ်မှတ်သည်။

### `a.X`

`a`ဒြပ်စင်သို့ အသုံးချပြီး ပိတ်ခလုတ်ပုံစံကို သတ်မှတ်သည်။