# සාමාන්‍ය බොත්තම සහ රවුම් අයිකන බොත්තම

## පන්තියේ නම

- සාමාන්‍ය බොත්තම`.Btn`
- නිරූපක බොත්තම `.BtnC`
- ප්‍රාථමික උද්දීපන තත්ත්වය`.Btn.Main`

## භාවිතා කරන්න

මූලද්‍රව්‍යයට පන්ති නාමයක් එක් කරන්න:

```html
<!-- සාමාන්ය බොත්තම -->
<button class="Btn">තහවුරු කරන්න</button>

<!-- ප්රධාන ඉස්මතු -->
<button class="Btn Main">ඉදිරිපත් කරන්න</button>

<!-- සබැඳි බොත්තම -->
<a class="Btn" href="https://github.com">සබැඳි බොත්තම</a>
```

## අයිකන විලාසය

`BtnC` සඳහා නිරූපකය `background-image` හරහා අභිරුචිකරණය කළ හැක:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```