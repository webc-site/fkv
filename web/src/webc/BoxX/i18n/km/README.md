# បង្កើត និងបង្ហាញស្រទាប់លេចឡើងដោយប្រើប៊ូតុងបិទ

- ការហៅមុខងារដើម្បីបង្កើត និងបង្ហាញស្រទាប់លេចឡើង
- ប៊ូតុងបិទដែលភ្ជាប់មកជាមួយ ចុចដើម្បីបិទ និងលុបធាតុចេញពី DOM
- ចុច Esc ដើម្បីផ្ដោតលើប្រអប់បញ្ចូល ឬបិទស្រទាប់លេចឡើង

## ប្រើការបង្ហាញ

```html
<button>ចុចដើម្បីលេចឡើង</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "អត្ថបទ";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## ប៉ារ៉ាម៉ែត្រចំណុចប្រទាក់

មុខងារនាំចេញលំនាំដើមត្រឡប់ `HTMLDialogElement` ហើយថ្នាក់រចនាប័ទ្ម `Box` និង `X` ត្រូវបានបន្ថែមទៅធាតុប្រអប់។

## ថ្នាក់រចនាប័ទ្ម

### `.Box.X`

បាន​អនុវត្ត​ទៅ​ធាតុ `dialog` បញ្ជាក់​រចនាប័ទ្ម​ប្រអប់។

### `a.X`

បានអនុវត្តទៅធាតុ `a` បញ្ជាក់រចនាប័ទ្មប៊ូតុងបិទ។