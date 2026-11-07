# ប៊ូតុងធម្មតា និងប៊ូតុងរូបតំណាងមូល

## ឈ្មោះថ្នាក់

- ប៊ូតុងធម្មតា`.Btn`
- ប៊ូតុងរូបតំណាង `.BtnC`
- ស្ថានភាពរំលេចបឋម`.Btn.Main`

## ប្រើ

បន្ថែមឈ្មោះថ្នាក់ទៅធាតុ៖

```html
<!-- ប៊ូតុងធម្មតា។ -->
<button class="Btn">បញ្ជាក់</button>

<!-- ការបន្លិចសំខាន់ -->
<button class="Btn Main">ដាក់ស្នើ</button>

<!-- ប៊ូតុងភ្ជាប់ -->
<a class="Btn" href="https://github.com">ប៊ូតុងភ្ជាប់</a>
```

## រចនាប័ទ្មរូបតំណាង

រូបតំណាងសម្រាប់ `BtnC`អាចប្ដូរតាមបំណងតាមរយៈ `background-image`៖

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```