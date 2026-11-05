# ການດີບັກ HTML ແລະລະຫັດ JS ອອນລາຍ

- ສະແດງແຖບດີບັກຢູ່ລຸ່ມລະຫັດ ແລະໃຫ້ CodePen ແລະ JSFiddle ປະຕູດີບັກ
- ຈື່ອັດຕະໂນມັດເວທີການດີບັກທີ່ເລືອກ
- ສະ​ຫນັບ​ສະ​ຫນູນ injecting callbacks ຜ່ານ `.gen` ຄຸນ​ລັກ​ສະ​ນະ​ເພື່ອ​ສ້າງ​ແບບ​ເຄື່ອນ​ໄຫວ​ລະ​ຫັດ HTML, CSS ແລະ JS
- ລະບຸອົງປະກອບຂອງເດັກນ້ອຍໂດຍອັດຕະໂນມັດດ້ວຍ `language-js`ຊື່ຊັ້ນຮຽນ ແລະແລ່ນໃນໂໝດ JS

## ໃຊ້ຕົວຢ່າງ

```html
<c-htm-demo><pre><code class="language-html">
<h1>ສາທິດ</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // ກັບຄືນ [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## ຕົວກໍານົດການໃນການໂຕ້ຕອບ

### ຄຸນສົມບັດແລະວິທີການ

- `.gen`: ຟັງຊັນໂທກັບທີ່ໃສ່ແລ້ວ. **ການ​ໂທ​ກັບ​ຄືນ​ນີ້​ຕ້ອງ​ໄດ້​ຮັບ​ການ​ສີດ​ສໍາ​ລັບ​ແຖບ​ແກ້​ໄຂ​ບັນ​ຫາ​ທີ່​ຈະ​ສະ​ແດງ​. **
  - ພາລາມິເຕີ:
    - `type`: ປະເພດເນື້ອໃນສີດ (`HTML = 1`, `JS = 2`).
    - `text`: ຂໍ້ຄວາມລະຫັດທີ່ແຍກອອກຈາກອົງປະກອບລູກ.
  - ຄ່າກັບຄືນ: `[htm, css, js]`