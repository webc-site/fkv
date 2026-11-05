# drop-down ရွေးချယ်မှုအကွက်နှင့် ခလုတ်ကို ပေါင်းစပ်ထားသည်။

- ရွေးချယ်မှုအကွက်ကို အလျားလိုက်ခွဲ၍ ခလုတ်ကို ခွဲပါ။
- ရွေးချယ်ထားသောအခြေအနေကို localStorage တွင်အလိုအလျောက်သိမ်းဆည်းပါ။
- ကြားဖြတ်ခလုတ်ကို နှိပ်ပြီး စိတ်ကြိုက်ဖြစ်ရပ်များကို ပေးပို့ပါ။

## ဒီမိုကိုသုံးပါ။

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">ရွေးချယ်မှု ၁</option>
    <option value="option2">ရွေးချယ်မှု ၂</option>
  </c-select>
  <button>ပြေး</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ပစ္စည်းဥစ္စာ

- `key`: localStorage မြဲမြံမှုအတွက် အသုံးပြုသော သော့အမည်
- `value`- လက်ရှိရွေးချယ်ထားသော တန်ဖိုး

## အဖြစ်အပျက်

- `submit`- ခလုတ်ကို နှိပ်လိုက်သောအခါ၊ `e.value` လက်ရှိရွေးချယ်ထားသော တန်ဖိုးကို သယ်ဆောင်သည်