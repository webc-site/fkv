# သာမန်ခလုတ်နှင့် အဝိုင်းပုံလေးခလုတ်

## အတန်းနာမည်

- ပုံမှန်ခလုတ်`.Btn`
- အိုင်ကွန်ခလုတ် `.BtnC`
- အဓိက အသားပေးဖော်ပြမှု အခြေအနေ`.Btn.Main`

## အသုံးပြု

ဒြပ်စင်သို့ အတန်းအမည်တစ်ခု ထည့်ပါ-

```html
<!-- ပုံမှန်ခလုတ် -->
<button class="Btn">အတည်ပြုပါ။</button>

<!-- အဓိကမီးမောင်းထိုးပြ -->
<button class="Btn Main">တင်ပြပါ။</button>

<!-- link ကိုနှိပ်ပါ။ -->
<a class="Btn" href="https://github.com">link ကိုနှိပ်ပါ။</a>
```

## အိုင်ကွန်စတိုင်

`BtnC`အတွက် အိုင်ကွန်ကို `background-image`မှတဆင့် စိတ်ကြိုက်ပြင်ဆင်နိုင်သည်-

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```